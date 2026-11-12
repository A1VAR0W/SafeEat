# Copied into gen/android/app by the builders (gen/ is generated, not tracked).
# R8 broke ML Kit in release builds: the barcode scanner crashed with a
# NullPointerException in com.google.android.gms.internal.mlkit_vision_common as soon
# as the camera delivered its first frame. Keep ML Kit and its internal vision code.
-keep class com.google.mlkit.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_common.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_barcode.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_barcode_bundled.** { *; }
-dontwarn com.google.mlkit.**
