#!/usr/bin/env bash

set -euo pipefail

if [ ! -d ".git" ]; then
  echo "must be run from the root of the repository"
  exit 1
fi

VULKAN_VERSION=1.4.335.0
VULKAN_SDK_URL="https://sdk.lunarg.com/sdk/download/${VULKAN_VERSION}/linux/vulkansdk-linux-x86_64-${VULKAN_VERSION}.tar.xz"

if [[ ! -d "vendor/vulkan/${VULKAN_VERSION}" ]]; then
  mkdir -p vendor/vulkan
  curl -L -o vendor/vulkan/vulkan.tar.xz "${VULKAN_SDK_URL}"
  tar -xvf vendor/vulkan/vulkan.tar.xz -C vendor/vulkan
else
  echo "vulkan sdk ${VULKAN_VERSION} already downloaded"
fi

echo "run \`source source-me.sh\` to setup the environment"
