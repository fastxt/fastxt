BIN_PATH=$HOME/Android/Sdk/ndk-bundle/toolchains/llvm/prebuilt/linux-x86_64/bin
cd `dirname $0`/../fastxt-rs/fastxt_core
# cargo clean
PATH=$BIN_PATH:$PATH cargo build --target armv7-linux-androideabi --release
PATH=$BIN_PATH:$PATH cargo build --target i686-linux-android --release
PATH=$BIN_PATH:$PATH cargo build --target aarch64-linux-android --release
PATH=$BIN_PATH:$PATH cargo build --target x86_64-linux-android --release

