# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# --- Свиток: классы, вызываемые рефлексией/JNI, нельзя вырезать/переименовывать ---
# MainActivity (загружается по имени из манифеста) и Kotlin-плагин Keystore
# (обнаруживается рантаймом Tauri по аннотациям @TauriPlugin/@Command).
-keep class app.svitok.vault.** { *; }
-keep @app.tauri.annotation.TauriPlugin class * { *; }
-keepclassmembers class * {
    @app.tauri.annotation.Command <methods>;
    @app.tauri.annotation.PermissionCallback <methods>;
}
# JS-интерфейс WebView (__svitokAndroid.exit) вызывается из JavaScript.
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}