# Manual Linux release

There is no GitHub Actions pipeline. Do not publish development pushes automatically or build macOS/Windows artifacts. The initial supported binary architecture is **Linux x86_64**; ARM builds are not yet validated.

1. Review CHANGELOG.md, update Cargo.toml and Cargo.lock to the same version, and run `scripts/check.sh` on Linux. Inspect `git diff` and `git ls-files`: no caches, secrets, packages or review media. Refresh third-party notices with `python3 scripts/licenses.py` after dependency changes.
2. Build the official executable on the pinned Debian 12 / Rust 1.93.1 image. Do not release a binary linked against a newer host glibc. Use a temporary directory outside the checkout; all container bind paths must be absolute:

   ```sh
   docker build -t linux-task-manager-builder -f packaging/Dockerfile .
   mkdir -p /tmp/ltm-release/build /tmp/ltm-release/cargo /tmp/ltm-release/dist
   docker run --rm --user "$(id -u):$(id -g)" \
     -v "$PWD:/source:ro" -v /tmp/ltm-release/cargo:/cargo \
     -v /tmp/ltm-release/build:/build linux-task-manager-builder
   ```

3. Obtain [appimagetool 1.9.1](https://github.com/AppImage/appimagetool/releases/tag/1.9.1), Linux x86_64. Before executing it, verify SHA-256 `ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0`. Keep the tool outside the checkout. Also download `runtime-x86_64` from [type2-runtime continuous](https://github.com/AppImage/type2-runtime/releases/tag/continuous), commit `fedfad51acc2860b2916a1eef32ba4a500ff31ef`, and verify SHA-256 `617aa77314d57c8b8e4fae9c3f0b35af7b30d5529cb69e98f64f92510ed98a46`. If the moving download changes, retrieve that exact runtime or explicitly review and update its checksum/source/license together. The packaging script verifies the runtime and embeds it without downloading a newer one. Package the baseline binary:

   ```sh
   APPIMAGETOOL=/tmp/appimagetool-x86_64.AppImage APPIMAGE_RUNTIME=/tmp/runtime-x86_64 scripts/package-linux.sh \
     /tmp/ltm-release/dist /tmp/ltm-release/build/release/linux-task-manager
   ```

4. Test the **packaged** executable: `--version`, `--self-test`, normal startup, demo screenshots and low-memory startup. Extract the .deb and tar.gz and compare their executable hashes with the build; run the AppImage with `--appimage-extract-and-run --version` if FUSE is unavailable. Confirm the maximum ELF GLIBC requirement is no higher than 2.36. Install/remove tests belong in a disposable container, not the user's desktop.
5. Generate the README animation with `BINARY=/tmp/ltm-release/build/release/linux-task-manager scripts/capture-demo.sh /tmp/ltm-release/tour`, inspect every frame and attach `demo-tour.gif` to the release. The tour covers every navigation page, all Performance resources and Application details. Capture public screenshots with `--demo --language en --screenshot /tmp/ltm-release/dist/demo-processes.png` (add `--page performance --resource cpu` for another view). Inspect every image for private data. Store images as **release attachments**, never Git files. Update README image links if the screenshot release changes. Include media in SHA256SUMS.txt alongside the packages before upload.
6. Commit reviewed source to main and push without force. Verify origin matches the local commit, then tag that commit `vVERSION` and push the tag. Create a **draft** GitHub Release targeting the tag, with the Linux packages, demo media and SHA256SUMS.txt. Use a temporary file with `gh release create --notes-file` for multiline notes. Describe installation, architecture, glibc baseline, unsigned packages, optional Polkit/NVIDIA requirements and known limitations.
7. Download the draft's assets to a fresh temporary directory, verify `sha256sum -c SHA256SUMS.txt`, compare the draft tag and source commit, and publish the verified draft. Fetch its metadata again and confirm it is public and has only the intended assets. Never overwrite a published version.
8. Check `git status`, verify every tracked file is in the commit on origin, and remove only disposable work from this release. Preserve unrelated changes, local-only work, app caches used for development, and external review evidence. Report leftovers clearly.

For 0.0.1 through 0.0.3, the baseline is Debian 12 (glibc 2.36), also suitable for Ubuntu 24.04+ with the listed graphics libraries. An AppImage is a convenient executable container; it does **not** eliminate the graphics-library or glibc requirements. It requires FUSE mounting support; `--appimage-extract-and-run` is the fallback. A fresh environment still needs its own graphics driver.
