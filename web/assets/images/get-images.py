# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

import os
import requests
from pathlib import Path

# Repository details
OWNER = "opalkelly-opensource"
REPO = "XilinxBoardStore"
BRANCH = "master"
TARGET_DIR = "boards/OpalKelly"
OUTPUT_FOLDER = "./all_downloaded_images"

# 1. Fetch recursive repository tree via GitHub API
tree_api_url = f"https://api.github.com/repos/{OWNER}/{REPO}/git/trees/{BRANCH}?recursive=1"
headers = {"User-Agent": "Python-JPG-Downloader"}

# Optional: Add GitHub Token for rate-limiting bypass if needed
# headers["Authorization"] = "token YOUR_GITHUB_TOKEN"

print("Fetching repository tree...")
response = requests.get(tree_api_url, headers=headers)
response.raise_for_status()

tree_data = response.json().get("tree", [])

# 2. Filter for JPG / JPEG files under boards/OpalKelly
jpg_files = [
    item["path"]
    for item in tree_data
    if item.get("type") == "blob"
    and item["path"].startswith(TARGET_DIR)
    and item["path"].lower().endswith((".jpg", ".jpeg"))
]

print(f"Found {len(jpg_files)} JPG file(s). Starting download...\n")

# Ensure flat output folder exists
output_path = Path(OUTPUT_FOLDER)
output_path.mkdir(parents=True, exist_ok=True)

raw_base_url = f"https://raw.githubusercontent.com/opalkelly-opensource/{REPO}/{BRANCH}/"

for file_path in jpg_files:
    raw_url = raw_base_url + file_path
    
    # Generate a unique flat filename using parent folder path segments
    # e.g., 'boards/OpalKelly/xem8320/1.0/board.jpg' -> 'xem8320_1.0_board.jpg'
    rel_path = os.path.relpath(file_path, TARGET_DIR)
    flat_filename = rel_path.replace(os.sep, "_").replace("/", "_")
    
    destination = output_path / flat_filename

    print(f"Downloading: {file_path} -> {flat_filename}")
    
    file_response = requests.get(raw_url, headers=headers)
    if file_response.status_code == 200:
        with open(destination, "wb") as f:
            f.write(file_response.content)
    else:
        print(f"Failed to download {file_path} (Status code: {file_response.status_code})")

print(f"\nAll downloads completed. All images saved in: '{output_path.resolve()}'")