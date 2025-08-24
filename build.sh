#!/bin/sh

echo "[INFO] Starting plugin assembly"

print_err() {
    echo "[ERROR] $1"
    exit 1
}

PLUGIN_NAME="gitvol"
VERSION=$(cargo run --quiet -- --version | awk '{split($0,a," "); print a[2]}') || print_err "Failed to get version from cargo"
BUILD_IMAGE="${PLUGIN_NAME}_rootfs_image"
BUILD_PATH="$PWD/build"
ROOTFS_PATH="$BUILD_PATH/rootfs"

clean_rootfs_image() {
    echo "[INFO] Cleaning old rootfs image: $BUILD_IMAGE"
    exists_image_id=$(docker images "$BUILD_IMAGE" -q)
    if [ -n "$exists_image_id" ]; then
        echo "[INFO] Found image ID: $exists_image_id"
        
        runned_containers=$(docker ps -q --filter "ancestor=$BUILD_IMAGE")
        if [ -n "$runned_containers" ]; then
            echo "[INFO] Stopping running containers: $runned_containers"
            docker stop $runned_containers || print_err "Failed to stop containers"
        fi
        
        containers=$(docker ps -qa --filter "ancestor=$BUILD_IMAGE")
        if [ -n "$containers" ]; then
            echo "[INFO] Removing containers: $containers"
            docker rm -vf $containers || print_err "Failed to remove containers"
        fi
        
        echo "[INFO] Removing old image: $exists_image_id"
        docker rmi -f "$exists_image_id" || print_err "Failed to remove image"
    else
        echo "[INFO] No old image found"
    fi
}

build_rootfs_image() {
    echo "build docker image: $BUILD_IMAGE"
    clean_rootfs_image
    docker build -t ${BUILD_IMAGE} . || print_err "Failed to build image"
}

clean_rootfs() {
    if [ -d "$ROOTFS_PATH" ]; then
        echo "[INFO] Cleaning rootfs directory: $ROOTFS_PATH"
        rm -rf $ROOTFS_PATH || print_err "Failed to clean rootfs"
    else
        echo "[INFO] No rootfs directory to clean"
    fi
}

update_rootfs() {
    echo "[INFO] Checking and updating rootfs"
    clean_rootfs
    mkdir $ROOTFS_PATH || print_err "Failed to create rootfs directory"
}

clean_plugin() {
    pn="${PLUGIN_NAME}:${1}"
    echo "[INFO] Cleaning plugin: $pn"
    plugin_info=$(docker plugin ls | grep "$pn")
    if [ -n "$plugin_info" ]; then
        echo "[INFO] Plugin $pn already exists"
        plugin_enabled=$(echo $plugin_info | awk '{split($0,a," "); print a[4]}')
        if [ "$plugin_enabled" = "true" ]; then
            echo "[INFO] Disabling enabled plugin: $pn"
            docker plugin disable -f "$pn" || print_err "Failed to disable plugin"
        fi
        echo "[INFO] Removing plugin: $pn"
        docker plugin rm -f "$pn" || print_err "Failed to remove plugin"
    else
        echo "[INFO] No plugin found: $pn"
    fi
}

create_plugin() {
    echo "\n[INFO] building plugin $PLUGIN_NAME with tag $1"
    clean_plugin $1
    update_rootfs
    echo "[INFO] Creating container from image: $BUILD_IMAGE"
    ID=$(docker create ${BUILD_IMAGE}) || print_err "Failed to create container"
    echo "[INFO] Container ID: $ID"
    echo "[INFO] Exporting rootfs"
    docker export $ID | tar -x -C $ROOTFS_PATH || print_err "Failed to export rootfs"
    docker rm -vf $ID || print_err "Failed to remove ($ID) container"
    echo "[INFO] Creating plugin: ${PLUGIN_NAME}:${1}"
    docker plugin create "${PLUGIN_NAME}:${1}" "$BUILD_PATH" || print_err "Failed to create plugin"
}



build_rootfs_image
create_plugin $VERSION
create_plugin latest
clean_rootfs_image
clean_rootfs

echo "[INFO] Plugin assembly completed successfully"
docker plugin ls
