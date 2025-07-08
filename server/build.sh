echo "Initializing build tools"

if [[ $# -eq 0 || "$1" == "help" ]]; then
    echo "Usage: ./build.sh [init|help|<target>|all]"
    echo ""
    echo "  init: Clone the cross repository and build the Docker images for cross compilation. Required before any build."
    echo "  help: Show this help message."
    echo "  <target>: Build the specified target using cross. Example: x86_64-apple-darwin. Valid targets are:"
    echo "    x86_64-pc-windows-msvc"
    echo "    aarch64-pc-windows-msvc"
    echo "    x86_64-unknown-linux-gnu"
    echo "    aarch64-unknown-linux-gnu"
    echo "    x86_64-unknown-linux-musl"
    echo "    aarch64-unknown-linux-musl"
    echo "    x86_64-apple-darwin"
    echo "    aarch64-apple-darwin"
    echo "  all: Build all targets."
    read -n 1 -s -r -p "Press any key to close..."
    exit 0
fi

# check docker installation
if ! command -v docker &> /dev/null; then
    echo "Docker not installed. Please install Docker to continue."
    read -n 1 -s -r -p "Press any key to close..."
    exit 1
fi
if ! docker info &> /dev/null; then
    echo "Docker not running. Please check that docker is ready and has suffisent privileges."
    read -n 1 -s -r -p "Press any key to close..."
    exit 2
fi

#init
#clone cross if init is passed as first argument
if [ "$1" == "init" ]; then

    echo "Checking requirements"

    echo "Cloning cross repository"
    git clone https://github.com/cross-rs/cross
    cd cross
    git submodule update --init --remote
    cargo install --path .
    cargo build-docker-image x86_64-pc-windows-msvc-cross --tag local
    cargo build-docker-image aarch64-pc-windows-msvc-cross --tag local
    if [[ -f "docker/MacOSX15.sdk.tar.xz" ]]; then
        echo "MacOSX15.sdk.tar.xz found, using it for cross compilation"
    else
        echo "MacOSX15.sdk.tar.xz not found, please download it and place it in cross/docker directory."
        read -n 1 -s -r -p "Press any key to close..."
        exit 1
    fi
    cargo build-docker-image aarch64-apple-darwin-cross --build-arg 'MACOS_SDK_FILE=MacOSX15.sdk.tar.xz' --tag local
    cargo build-docker-image x86_64-apple-darwin-cross --build-arg 'MACOS_SDK_FILE=MacOSX15.sdk.tar.xz' --tag local

    echo ""
    echo "Docker images are ready. You can now build different targets using build.sh without arguments or with the target as argument."
    read -n 1 -s -r -p "Press any key to close..."
    exit 0
fi

if [[ "$1" == "all" || "$1" == "x86_64-pc-windows-msvc" ]]; then
    echo "Building win32-x64 release"
    cross build -r --target x86_64-pc-windows-msvc
    x86_64_pc_windows_gnu_result=$?
fi

if [[ "$1" == "all" || "$1" == "aarch64-pc-windows-msvc" ]]; then
    echo "Building win32-arm64 release"
    cross build -r --target aarch64-pc-windows-msvc
    aarch64_pc_windows_msvc_result=$?
fi

if [[ "$1" == "all" || "$1" == "x86_64-unknown-linux-gnu" ]]; then
    echo "Building linux-x64 release"
    cross build -r --target x86_64-unknown-linux-gnu
    x86_64_unknown_linux_gnu_result=$?
fi

if [[ "$1" == "all" || "$1" == "aarch64-unknown-linux-gnu" ]]; then
    echo "Building linux-arm64 release"
    cross build -r --target aarch64-unknown-linux-gnu
    aarch64_unknown_linux_gnu_result=$?
fi

if [[ "$1" == "all" || "$1" == "x86_64-unknown-linux-musl" ]]; then
    echo "Building alpine-x64 (musl) release"
    cross build -r --target x86_64-unknown-linux-musl
    x86_64_unknown_linux_musl_result=$?
fi

if [[ "$1" == "all" || "$1" == "aarch64-unknown-linux-musl" ]]; then
    echo "Building alpine-arm64 (musl) release"
    cross build -r --target aarch64-unknown-linux-musl
    aarch64_unknown_linux_musl_result=$?
fi

if [[ "$1" == "all" || "$1" == "x86_64-apple-darwin" ]]; then
    echo "Building x86_64-darwin release"
    cross build -r --target x86_64-apple-darwin
    x86_64_apple_darwin_result=$?
fi

if [[ "$1" == "all" || "$1" == "aarch64-apple-darwin" ]]; then
    echo "Building aarch64-darwin release"
    cross build -r --target aarch64-apple-darwin
    aarch64_apple_darwin_result=$?
fi

#check results
if [[ "$1" == "all" || "$1" == "x86_64-pc-windows-msvc" ]]; then
    if [[ $x86_64_pc_windows_gnu_result -eq 0 ]]; then
        echo "Target x86_64-pc-windows-msvc built successfully."
    else
        echo "Target x86_64-pc-windows-msvc FAILED."
    fi
fi
if [[ "$1" == "all" || "$1" == "aarch64-pc-windows-msvc" ]]; then
    if [[ $aarch64_pc_windows_msvc_result -eq 0 ]]; then
        echo "Target aarch64-pc-windows-msvc built successfully."
    else
        echo "Target aarch64-pc-windows-msvc FAILED."
    fi
fi
if [[ "$1" == "all" || "$1" == "x86_64-unknown-linux-gnu" ]]; then
    if [[ $x86_64_unknown_linux_gnu_result -eq 0 ]]; then
        echo "Target x86_64-unknown-linux-gnu built successfully."
    else
        echo "Target x86_64-unknown-linux-gnu FAILED."
    fi
fi
if [[ "$1" == "all" || "$1" == "aarch64-unknown-linux-gnu" ]]; then
    if [[ $aarch64_unknown_linux_gnu_result -eq 0 ]]; then
        echo "Target aarch64-unknown-linux-gnu built successfully."
    else
        echo "Target aarch64-unknown-linux-gnu FAILED."
    fi
fi
if [[ "$1" == "all" || "$1" == "x86_64-unknown-linux-musl" ]]; then
    if [[ $x86_64_unknown_linux_musl_result -eq 0 ]]; then
        echo "Target x86_64-unknown-linux-musl built successfully."
    else
        echo "Target x86_64-unknown-linux-musl FAILED."
    fi
fi
if [[ "$1" == "all" || "$1" == "aarch64-unknown-linux-musl" ]]; then
    if [[ $aarch64_unknown_linux_musl_result -eq 0 ]]; then
        echo "Target aarch64-unknown-linux-musl built successfully."
    else
        echo "Target aarch64-unknown-linux-musl FAILED."
    fi
fi
if [[ "$1" == "all" || "$1" == "x86_64-apple-darwin" ]]; then
    if [[ $x86_64_apple_darwin_result -eq 0 ]]; then
        echo "Target x86_64-apple-darwin built successfully."
    else
        echo "Target x86_64-apple-darwin FAILED."
    fi
fi
if [[ "$1" == "all" || "$1" == "aarch64-apple-darwin" ]]; then
    if [[ $aarch64_apple_darwin_result -eq 0 ]]; then
        echo "Target aarch64-apple-darwin built successfully."
    else
        echo "Target aarch64-apple-darwin FAILED."
    fi
fi

read -n 1 -s -r -p "Press any key to close..."