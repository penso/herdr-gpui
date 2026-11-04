#!/usr/bin/env bash
# Install each distribution package into a clean container of its distribution,
# resolve its declared dependencies from that distribution's repositories, and
# check that the installed executable finds every library it links or dlopens.
# `--help` exits before GPUI starts, so no display is needed.
#
# Runs the host's architecture. Official Arch Linux images exist only for
# x86_64, so the ARM64 Arch package is built but not installed here.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# == 3 ]] || fail 'Usage: bash smoke-linux-packages.sh VERSION TARGET DIST_DIR'
version_check "$1"
case $2 in x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu) ;; *) fail 'Unsupported Linux target';; esac
dist=$(cd -- "$3" && pwd)
name=Sheprd-$1-$2
command -v docker >/dev/null || fail 'docker is required'

# Index digests pin each image across architectures; bump them together.
ubuntu=ubuntu:24.04@sha256:008173c23f95b170204355c12626cb5a965d779a7e1283b09e9cffbb1bf33ca3
debian=debian:trixie@sha256:9cc080028c43b27d2074d63a5f9caf7166d731494965616c1a6d2827a004585c
fedora=fedora:42@sha256:99e203b80b1c3d8f7e161ec10a68fd02b081ef83a3963553e513c82846b97814
arch=archlinux:latest@sha256:f3691b4dde62ba4c4b6f0ae2c1fbf28e8c0c8c4b9a35c7e06dc1f70e21aa29f6

# Shared checks, run inside every container after installation; expanded there.
# shellcheck disable=SC2016
check='
set -euo pipefail
/usr/bin/sheprd --help >/dev/null
/usr/bin/herdr-gpui --help >/dev/null
test -f /usr/bin/herdr-gpui && test ! -L /usr/bin/herdr-gpui
test "$(stat -c %u /usr/bin/herdr-gpui)" = 0
cmp /usr/bin/sheprd /usr/bin/herdr-gpui
for helper in acp automate remote-control nearby; do
    test -f "/usr/bin/herdr-$helper" && test ! -L "/usr/bin/herdr-$helper"
    cmp "/usr/bin/sheprd-$helper" "/usr/bin/herdr-$helper"
done
/usr/bin/sheprd-acp --help >/dev/null
/usr/bin/sheprd-remote-control --version >/dev/null
/usr/bin/sheprd-automate --version >/dev/null
/usr/bin/sheprd-nearby --version >/dev/null
/usr/bin/herdr-core --version >/dev/null
# Capture before matching: with pipefail, grep -q exiting early would fail the
# writer with SIGPIPE and turn a present library into a false failure.
# herdr-core is a static-pie binary with no libraries to find, so it is left
# out: the ldd output for those differs between glibc releases.
for program in sheprd herdr-gpui sheprd-acp sheprd-remote-control sheprd-automate sheprd-nearby; do
    linked=$(ldd "/usr/bin/$program")
    if grep "not found" <<<"$linked"; then exit 1; fi
done
cache=$(ldconfig -p)
for library in libvulkan.so.1 libwayland-client.so.0; do
    grep -q "^[[:space:]]*$library " <<<"$cache" || { echo "missing dlopen target $library" >&2; exit 1; }
done
test -f /usr/share/applications/so.pen.herdr-gpui.desktop
test -f /usr/share/icons/hicolor/scalable/apps/sheprd.svg
test -f /usr/share/licenses/herdr-gpui/THIRD-PARTY-NOTICES.txt
'
removed='
for program in sheprd herdr-gpui herdr-core; do test ! -e "/usr/bin/$program"; done
for prefix in sheprd herdr; do
    for helper in acp automate remote-control nearby; do
        test ! -e "/usr/bin/$prefix-$helper"
    done
done
'

run() {
    printf '== %s: %s\n' "$1" "$2"
    docker run --rm --pull=missing -v "$dist:/dist:ro" -e CHECK="$check" -e REMOVED="$removed" "$2" bash -euo pipefail -c "$3"
}

for image in "$ubuntu" "$debian"; do
    run "$name.deb" "$image" "
        apt-get update -qq
        DEBIAN_FRONTEND=noninteractive apt-get install -qq -y --no-install-recommends /dist/$name.deb >/dev/null
        bash -c \"\$CHECK\"
        test \"\$(dpkg-query -W -f='\${Version}' herdr-gpui)\" = '$1-1'
        test \"\$(dpkg-query -S /usr/bin/herdr-gpui)\" = 'herdr-gpui: /usr/bin/herdr-gpui'
        test \"\$(dpkg-query -S /usr/bin/sheprd)\" = 'herdr-gpui: /usr/bin/sheprd'
        apt-get remove -qq -y herdr-gpui >/dev/null
        bash -c \"\$REMOVED\""
done
run "$name.rpm" "$fedora" "
    # The minimal Fedora image lacks cmp, used to verify compatibility copies.
    dnf install -q -y --setopt=install_weak_deps=False diffutils /dist/$name.rpm >/dev/null
    bash -c \"\$CHECK\"
    test \"\$(rpm -q --qf '%{VERSION}-%{RELEASE}' herdr-gpui)\" = '$1-1'
    test \"\$(rpm -qf --qf '%{NAME}' /usr/bin/herdr-gpui)\" = herdr-gpui
    dnf remove -q -y herdr-gpui >/dev/null
    bash -c \"\$REMOVED\""
if [[ $2 == x86_64-unknown-linux-gnu ]]; then
    run "$name.pkg.tar.zst" "$arch" "
        # pacman 7 drops to a seccomp-sandboxed download user, which some
        # container runtimes refuse; this throwaway container needs neither.
        pacman -Syu --noconfirm --needed --disable-sandbox >/dev/null
        pacman -U --noconfirm --disable-sandbox /dist/$name.pkg.tar.zst >/dev/null
        bash -c \"\$CHECK\"
        test \"\$(pacman -Q herdr-gpui)\" = 'herdr-gpui $1-1'
        test \"\$(pacman -Qoq /usr/bin/herdr-gpui)\" = herdr-gpui
        pacman -R --noconfirm herdr-gpui >/dev/null
        bash -c \"\$REMOVED\""
fi
