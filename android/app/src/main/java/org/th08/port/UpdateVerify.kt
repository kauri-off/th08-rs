// In-app update checks without Android types: SHA256SUMS lookup, the release-channel gate, the APK identity rules.
package org.th08.port

import java.security.MessageDigest

// `sha256sum` lines (`<hex>  <name>`, `<hex> *<name>` in binary mode) → name to lowercase hex; other lines are skipped.
fun parseSums(text: String): Map<String, String> = buildMap {
    for (line in text.lineSequence()) {
        val m = SUM_LINE.matchEntire(line.trimEnd('\r')) ?: continue
        putIfAbsent(m.groupValues[2], m.groupValues[1].lowercase())
    }
}

private val SUM_LINE = Regex("""([0-9A-Fa-f]{64}) [ *](\S.*)""")

// The checksum SHA256SUMS lists for `asset`, null when it lists none.
fun expectedSum(sums: String, asset: String): String? = parseSums(sums)[asset]

fun sumMatches(sums: String, asset: String, actualHex: String): Boolean =
    expectedSum(sums, asset)?.equals(actualHex, ignoreCase = true) == true

fun hex(b: ByteArray): String = b.joinToString("") { "%02x".format(it) }

fun sha256Hex(b: ByteArray): String = hex(MessageDigest.getInstance("SHA-256").digest(b))

// The release channel installs updates itself; other channels only with update_test.json present (emulator tests).
fun inAppUpdateAllowed(channel: String, testJson: Boolean): Boolean = channel == "release" || testJson

enum class ApkProblem { PACKAGE, VERSION, SIGNER }

// The downloaded APK must be this app, newer, and signed by the installed app's current key; `apkSigners` includes the
// APK's rotation lineage, so a rotated key that names the old one still passes.
fun checkApk(
    pkg: String, versionCode: Long, signers: Set<String>,
    apkPkg: String?, apkVersionCode: Long, apkSigners: Set<String>,
): ApkProblem? = when {
    apkPkg != pkg -> ApkProblem.PACKAGE
    apkVersionCode <= versionCode -> ApkProblem.VERSION
    signers.isEmpty() || !apkSigners.containsAll(signers) -> ApkProblem.SIGNER
    else -> null
}
