# Source this file before OpenRemote builds; all restored tools are task-local.
export OPENREMOTE_TOOLCHAINS="/workspace/scratch/72f8c6db0182/toolchains"
export CARGO_HOME="$OPENREMOTE_TOOLCHAINS/cargo-home"
export GOCACHE="$OPENREMOTE_TOOLCHAINS/go-build-cache"
export GOMODCACHE="$OPENREMOTE_TOOLCHAINS/go-mod-cache"
export GOTOOLCHAIN=local
export PATH="$OPENREMOTE_TOOLCHAINS/rust-1.100.0/bin:$OPENREMOTE_TOOLCHAINS/llvm-mingw-20261104-ucrt-ubuntu-22.04-x86_64/bin:$OPENREMOTE_TOOLCHAINS/nsis/usr/bin:$OPENREMOTE_TOOLCHAINS/7zip-26.06:$OPENREMOTE_TOOLCHAINS/go1.27.4/go/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER="$OPENREMOTE_TOOLCHAINS/llvm-mingw-20261104-ucrt-ubuntu-22.04-x86_64/bin/x86_64-w64-mingw32-clang"
export CC_x86_64_pc_windows_gnullvm="$CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER"
export CXX_x86_64_pc_windows_gnullvm="$OPENREMOTE_TOOLCHAINS/llvm-mingw-20261104-ucrt-ubuntu-22.04-x86_64/bin/x86_64-w64-mingw32-clang++"
export AR_x86_64_pc_windows_gnullvm="$OPENREMOTE_TOOLCHAINS/llvm-mingw-20261104-ucrt-ubuntu-22.04-x86_64/bin/llvm-ar"
export NSISDIR="$OPENREMOTE_TOOLCHAINS/nsis/usr/share/nsis"
