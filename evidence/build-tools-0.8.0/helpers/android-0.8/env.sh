# Android build tools and the retained preview identity live under this task.
export OPENREMOTE_ANDROID_TOOLS="/workspace/scratch/72f8c6db0182/toolchains/android-0.8"
export JAVA_HOME="$OPENREMOTE_ANDROID_TOOLS/jdk-21.0.13+9"
export ANDROID_HOME="$OPENREMOTE_ANDROID_TOOLS/sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_USER_HOME="$OPENREMOTE_ANDROID_TOOLS/android-user"
export GRADLE_USER_HOME="$OPENREMOTE_ANDROID_TOOLS/gradle-user"
export PATH="$JAVA_HOME/bin:$OPENREMOTE_ANDROID_TOOLS/gradle-8.13/bin:$ANDROID_HOME/build-tools/35.0.0:$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
