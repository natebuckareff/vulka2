#!/usr/bin/env bash

VULKAN_VERSION=1.4.335.0

if [ ! -d ".git" ]; then
  echo "must be run from the root of the repository"
  exit 1
fi

if [ "${VULKAN_SDK_ENV_LOADED}" = "${VULKAN_VERSION}" ]; then
  return 0 2>/dev/null || exit 0
fi

source ./vendor/vulkan/${VULKAN_VERSION}/setup-env.sh
export VULKAN_SDK_ENV_LOADED="${VULKAN_VERSION}"
