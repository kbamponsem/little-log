buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:9.3.1")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.2.10")
        // Glance widgets are Compose, so the Compose compiler plugin is required
        // and must match the Kotlin version exactly. Without it `provideContent
        // { }` compiles as an ordinary lambda, and the widget dies at runtime
        // with NoSuchMethodError — which the launcher shows as "Can't show
        // content".
        classpath("org.jetbrains.kotlin:compose-compiler-gradle-plugin:2.2.10")
    }
}

allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

tasks.register("clean").configure {
    delete("build")
}

