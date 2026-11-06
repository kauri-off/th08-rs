// th08 APK: Compose launcher (LauncherActivity) + NativeActivity game in the ":game" process.
// Inputs as -P properties (`cargo xtask dist --target android-apk` sets them, xtask/src/android.rs):
//   th08.abis       comma list, arm64-v8a (default) / x86_64     th08.data      folder with th08.pak + thbgm.pak
//                                                                                (preferred) or th08.dat + thbgm.dat
//   th08.channel    release | dev (default) | debug: applicationId org.th08.port[.dev|.debug] and app label
//   th08.keystore   keystore.properties for release signing       th08.debugKey  debug keystore (default
//                   (storeFile relative to the file)                             ~/.local/share/th08-dev/debug.keystore)
//   th08.versionCode (1)   th08.versionName   th08.minSdk (26)   th08.profile (cargo profile, release)
//   th08.packs + th08.packIds   folder of translation packs + comma list of lang_xx ids to bundle (personal builds)
import java.util.Properties
import javax.inject.Inject

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

val repoDir: File = rootDir.parentFile
fun prop(name: String): String? = (findProperty(name) as String?)?.takeIf { it.isNotBlank() }

val abiList = (prop("th08.abis") ?: "arm64-v8a").split(',').map { it.trim() }.filter { it.isNotEmpty() }
val minSdkLevel = (prop("th08.minSdk") ?: "26").toInt()
val appChannel = (prop("th08.channel") ?: "dev").also {
    if (it !in listOf("release", "dev", "debug")) throw GradleException("th08.channel $it: release, dev or debug")
}
val appVersion = prop("th08.versionName") ?: "0.1.0"

// libth08.so per ABI through tools/android/cargo_android.sh (NDK clang).
abstract class CargoBuild : DefaultTask() {
    @get:Input abstract val abis: ListProperty<String>
    @get:Input abstract val profile: Property<String>
    @get:Input abstract val minSdk: Property<Int>
    @get:Input abstract val version: Property<String>
    @get:Input abstract val channel: Property<String>
    @get:Internal abstract val repo: DirectoryProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty
    @get:Inject abstract val execOps: ExecOperations

    init {
        // cargo is incremental itself.
        outputs.upToDateWhen { false }
    }

    @TaskAction
    fun run() {
        val root = repo.get().asFile
        execOps.exec {
            workingDir = root
            environment("TH08_ANDROID_PROFILE", profile.get())
            environment("TH08_ANDROID_MIN_SDK", minSdk.get().toString())
            // Read by build.rs for the launcher and port-menu version line.
            environment("TH08_VERSION", version.get())
            environment("TH08_CHANNEL", channel.get())
            commandLine(listOf(File(root, "tools/android/cargo_android.sh").path, outputDir.get().asFile.path) + abis.get())
        }
    }
}

// settings_schema.json from the Rust ENTRIES table; the Compose screen is built from it at run time.
abstract class SettingsSchema : DefaultTask() {
    @get:Internal abstract val repo: DirectoryProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty
    @get:Inject abstract val execOps: ExecOperations

    init {
        outputs.upToDateWhen { false }
    }

    @TaskAction
    fun run() {
        val root = repo.get().asFile
        val out = outputDir.get().asFile.also { it.mkdirs() }
        execOps.exec {
            workingDir = root
            commandLine("cargo", "run", "-q", "--release",
                "-p", "th08-tools", "--bin", "settings_schema", File(out, "settings_schema.json").path)
        }
    }
}

// Optional bundled game data: th08.pak / thbgm.pak (repack, ASSETS_FORMAT §3.5) or th08.dat / thbgm.dat (any letter
// case) copied as lowercase assets; the source folder is only read. Up to date while the two files are unchanged.
abstract class GameData : DefaultTask() {
    @get:InputFiles abstract val files: ConfigurableFileCollection
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun run() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()
        files.files.forEach { it.copyTo(File(out, it.name.lowercase())) }
    }
}

// Translation packs as assets/packs/lang_xx; the launcher copies them into filesDir/packs (LauncherActivity).
abstract class PackAssets : DefaultTask() {
    @get:InputFiles abstract val dirs: ConfigurableFileCollection
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun run() {
        val root = outputDir.get().asFile
        root.deleteRecursively()
        val out = File(root, "packs").also { it.mkdirs() }
        dirs.files.forEach { it.copyRecursively(File(out, it.name)) }
    }
}

fun packDirs(): List<File> {
    val dir = prop("th08.packs") ?: return emptyList()
    return (prop("th08.packIds") ?: "").split(',').map { it.trim() }.filter { it.isNotEmpty() }.map { id ->
        File(dir, id).also { if (!File(it, "pack.toml").isFile) throw GradleException("no pack.toml in $it") }
    }
}

// The pack pair when the folder has both packs, else the raw pair.
fun gameFiles(dir: String?): List<File> {
    dir ?: return emptyList()
    fun find(name: String) = File(dir).listFiles()?.firstOrNull { it.isFile && it.name.equals(name, ignoreCase = true) }
    val packs = listOf("th08.pak", "thbgm.pak").map { find(it) }
    if (packs.all { it != null }) return packs.filterNotNull()
    return listOf("th08.dat", "thbgm.dat").map { name -> find(name) ?: throw GradleException("no $name in $dir") }
}

// Release keystore from keystore.properties (values stay in Gradle memory), else the throwaway debug key.
fun keyConfig(): Map<String, Any> {
    prop("th08.keystore")?.let { path ->
        val f = File(path).absoluteFile
        if (!f.isFile) throw GradleException("no keystore properties file $path")
        val p = Properties().apply { f.inputStream().use { load(it) } }
        fun v(k: String) = p.getProperty(k)?.trim() ?: throw GradleException("$path: missing $k")
        val store = File(v("storeFile")).let { if (it.isAbsolute) it else File(f.parentFile, it.path) }
        if (!store.isFile) throw GradleException("keystore named by $path not found")
        return mapOf("file" to store, "storePass" to v("storePassword"), "alias" to v("keyAlias"), "keyPass" to v("keyPassword"))
    }
    // Shared by every checkout and worktree, so debug APKs from any of them update each other.
    val ks = File(prop("th08.debugKey") ?: File(System.getProperty("user.home"), ".local/share/th08-dev/debug.keystore").path).absoluteFile
    if (!ks.isFile) {
        ks.parentFile.mkdirs()
        val p = ProcessBuilder("keytool", "-genkeypair", "-keystore", ks.path, "-storepass", "android", "-keypass",
            "android", "-alias", "th08debug", "-keyalg", "RSA", "-keysize", "2048", "-validity", "10000",
            "-dname", "CN=th08 debug").redirectErrorStream(true).start()
        p.inputStream.readBytes()
        if (p.waitFor() != 0) throw GradleException("keytool failed for $ks")
    }
    return mapOf("file" to ks, "storePass" to "android", "alias" to "th08debug", "keyPass" to "android")
}

android {
    namespace = "org.th08.port"
    compileSdk = 36
    ndkVersion = "27.1.12297006"

    defaultConfig {
        applicationId = "org.th08.port"
        minSdk = minSdkLevel
        targetSdk = 36
        versionCode = (prop("th08.versionCode") ?: "1").toInt()
        versionName = appVersion
        ndk { abiFilters += abiList }
    }

    signingConfigs {
        create("th08") {
            val k = keyConfig()
            storeFile = k["file"] as File
            storePassword = k["storePass"] as String
            keyAlias = k["alias"] as String
            keyPassword = k["keyPass"] as String
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            signingConfig = signingConfigs.getByName("th08")
            if (appChannel != "release") applicationIdSuffix = ".$appChannel"
            resValue("string", "app_name", if (appChannel == "release") "th08" else "th08 $appChannel")
            buildConfigField("String", "CHANNEL", "\"$appChannel\"")
        }
        debug {
            applicationIdSuffix = ".debug"
            signingConfig = signingConfigs.getByName("th08")
            resValue("string", "app_name", "th08 debug")
            buildConfigField("String", "CHANNEL", "\"debug\"")
        }
    }

    buildFeatures {
        compose = true
        resValues = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // Stored .so (extractNativeLibs=false) and stored .dat / .pak assets (AAsset fds, ARCH_D §2.1, ASSETS_FORMAT §3.5).
    packaging { jniLibs { useLegacyPackaging = false } }
    androidResources { noCompress += listOf("dat", "pak") }

    lint { checkReleaseBuilds = false }
}

androidComponents {
    onVariants { v ->
        val cap = v.name.replaceFirstChar { it.uppercase() }
        val cargo = tasks.register<CargoBuild>("cargoBuild$cap") {
            abis.set(abiList)
            profile.set(prop("th08.profile") ?: "release")
            minSdk.set(minSdkLevel)
            version.set(appVersion)
            channel.set(if (v.buildType == "debug") "debug" else appChannel)
            repo.set(repoDir)
        }
        val schema = tasks.register<SettingsSchema>("settingsSchema$cap") { repo.set(repoDir) }
        val data = tasks.register<GameData>("gameData$cap") { files.from(gameFiles(prop("th08.data"))) }
        val packs = tasks.register<PackAssets>("packAssets$cap") { dirs.from(packDirs()) }
        v.sources.jniLibs?.addGeneratedSourceDirectory(cargo, CargoBuild::outputDir)
        v.sources.assets?.addGeneratedSourceDirectory(schema, SettingsSchema::outputDir)
        v.sources.assets?.addGeneratedSourceDirectory(data, GameData::outputDir)
        v.sources.assets?.addGeneratedSourceDirectory(packs, PackAssets::outputDir)
    }
}

dependencies {
    val bom = platform("androidx.compose:compose-bom:2025.08.00")
    implementation(bom)
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material3.adaptive:adaptive:1.1.0")
    implementation("androidx.compose.ui:ui-tooling-preview")
    debugImplementation("androidx.compose.ui:ui-tooling")
    implementation("androidx.compose.material:material-icons-core")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.2")
    testImplementation("junit:junit:4.13.2")
}
