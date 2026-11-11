# Keep native JNI methods
-keepclasseswithmembernames class * {
    native <methods>;
}

-keep class com.jyroxa.app.** { *; }
-keep class com.jyroxa.app.native.** { *; }
-keep class com.jyroxa.app.data.model.** { *; }

# Kotlin Serialization
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.SerializationKt
-keepclassmembers class * {
    *** Companion;
}
-keepclasseswithmembers class * {
    kotlinx.serialization.KSerializer serializer(...);
}
