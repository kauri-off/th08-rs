// JVM tests of the in-app update checks (UpdateVerify.kt).
package org.th08.port

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class UpdateVerifyTest {
    private val apk = "th08-0.2.0-android-apk.apk"
    private val a = "a".repeat(64)
    private val b = "B".repeat(64)
    private val sums = """
        ${a}  th08-0.2.0-windows-exe.exe
        ${b} *$apk
        not a line
        ${"c".repeat(63)}  short.bin
        ${"d".repeat(64)}  th08-0.2.0-linux-appimage.AppImage
    """.trimIndent().replace("\n", "\r\n")

    @Test
    fun parse() {
        val m = parseSums(sums)
        assertEquals(setOf("th08-0.2.0-windows-exe.exe", apk, "th08-0.2.0-linux-appimage.AppImage"), m.keys)
        assertEquals("b".repeat(64), m[apk])
        assertEquals(a, expectedSum(sums, "th08-0.2.0-windows-exe.exe"))
        assertNull(expectedSum(sums, "short.bin"))
        assertNull(expectedSum(sums, "th08-0.2.0-android-apk"))
        assertNull(expectedSum("", apk))
    }

    @Test
    fun match() {
        assertTrue(sumMatches(sums, apk, "b".repeat(64)))
        assertTrue(sumMatches(sums, apk, "B".repeat(64)))
        assertFalse(sumMatches(sums, apk, a))
        assertFalse(sumMatches(sums, "missing.apk", a))
        // The first line for a name wins.
        assertTrue(sumMatches("$a  x.apk\n$b  x.apk\n", "x.apk", a))
        assertEquals("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", sha256Hex(ByteArray(0)))
    }

    @Test
    fun gate() {
        assertTrue(inAppUpdateAllowed("release", false))
        assertFalse(inAppUpdateAllowed("dev", false))
        assertFalse(inAppUpdateAllowed("debug", false))
        assertTrue(inAppUpdateAllowed("debug", true))
    }

    @Test
    fun apkRules() {
        val k = setOf("k1")
        val p = "org.th08.port"
        assertNull(checkApk(p, 5, k, p, 6, k))
        assertNull(checkApk(p, 5, k, p, 6, setOf("k0", "k1")))
        assertEquals(ApkProblem.PACKAGE, checkApk(p, 5, k, "org.th08.port.debug", 6, k))
        assertEquals(ApkProblem.PACKAGE, checkApk(p, 5, k, null, 6, k))
        assertEquals(ApkProblem.VERSION, checkApk(p, 5, k, p, 5, k))
        assertEquals(ApkProblem.SIGNER, checkApk(p, 5, k, p, 6, setOf("k2")))
        assertEquals(ApkProblem.SIGNER, checkApk(p, 5, emptySet(), p, 6, emptySet()))
    }
}
