# Keep native JNI methods
-keepclasseswithmembernames class * {
    native <methods>;
}

-keep class com.swiftget.** { *; }
-keep class com.swiftget.app.native.** { *; }
-keep class com.swiftget.app.data.model.** { *; }

# Kotlin Serialization
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.SerializationKt
-keepclassmembers class * {
    *** Companion;
}
-keepclasseswithmembers class * {
    kotlinx.serialization.KSerializer serializer(...);
}
