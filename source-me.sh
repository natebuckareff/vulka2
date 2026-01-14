#!/usr/bin/env bash

VULKAN_VERSION=1.4.335.0

if [ ! -d ".git" ]; then
  echo "must be run from the root of the repository"
  exit 1
fi

source ./vendor/vulkan/${VULKAN_VERSION}/setup-env.sh
