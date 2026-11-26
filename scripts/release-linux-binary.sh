#!/bin/sh
# Checks of a Linux release binary, run by the `build` job of .github/workflows/release.yml
# (story:release-linux-glibc-floor-and-musl). Each mode prints one line and exits 0, or names
# what it refused and exits 1.
#
#   release-linux-binary.sh glibc-floor <binary> <floor>
#       the binary's newest GLIBC_ symbol version (objdump -T) is at most <floor>
#   release-linux-binary.sh static <binary>
#       file(1) calls the binary static, and readelf shows no INTERP and no NEEDED
#   release-linux-binary.sh runs-in <image@sha256:digest> <binary> <first line>
#       `<binary> --version` answers <first line> in that image, with no network and a
#       read-only root; the image's libc line goes to stderr
set -eu

usage() {
    echo "usage: $0 glibc-floor <binary> <floor> | static <binary> | runs-in <image@sha256:digest> <binary> <first line>" >&2
    exit 2
}

refuse() {
    echo "release-linux-binary: refused: $*" >&2
    exit 1
}

binary() {
    [ -f "$1" ] || refuse "$1 is not a file"
}

glibc_floor() {
    bin=$1
    floor=$2
    binary "$bin"
    versions=$(objdump -T "$bin" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/^GLIBC_//' | sort -u -V) || true
    [ -n "$versions" ] || refuse "$bin names no GLIBC_ symbol version; it is no glibc binary"
    above=""
    for version in $versions; do
        if [ "$version" != "$floor" ] &&
            [ "$(printf '%s\n%s\n' "$floor" "$version" | sort -V | tail -n 1)" = "$version" ]; then
            above="$above GLIBC_$version"
        fi
    done
    [ -z "$above" ] || refuse "$bin needs${above}, above the glibc $floor floor"
    newest=$(printf '%s\n' "$versions" | tail -n 1)
    echo "$bin: newest GLIBC_ symbol version $newest, floor $floor"
}

static() {
    bin=$1
    binary "$bin"
    described=$(file --brief "$bin")
    case $described in
        *"statically linked"* | *"static-pie linked"*) ;;
        *) refuse "$bin is not statically linked: $described" ;;
    esac
    if readelf --program-headers --wide "$bin" | grep -q INTERP; then
        refuse "$bin names a program interpreter"
    fi
    needed=$(readelf --dynamic --wide "$bin" | grep '(NEEDED)') || true
    [ -z "$needed" ] || refuse "$bin needs a shared library: $needed"
    echo "$bin: static, no program interpreter, no shared library ($described)"
}

runs_in() {
    image=$1
    bin=$2
    expected=$3
    binary "$bin"
    case $image in
        *@sha256:*) ;;
        *) refuse "image $image is not pinned by digest" ;;
    esac
    digest=${image##*@sha256:}
    [ "${#digest}" -eq 64 ] || refuse "image $image is not pinned by a sha256 digest"
    case $digest in
        *[!0-9a-f]*) refuse "image $image is not pinned by a sha256 digest" ;;
    esac
    dir=$(cd "$(dirname "$bin")" && pwd)
    name=$(basename "$bin")
    answer=$(${DOCKER:-docker} run --rm --network none --read-only \
        --volume "$dir:/release:ro" "$image" \
        sh -c '{ ldd --version 2>&1 | head -n 1; } >&2; exec "$@"' sh "/release/$name" --version) ||
        refuse "$bin does not answer --version in $image"
    first=$(printf '%s\n' "$answer" | head -n 1)
    [ "$first" = "$expected" ] || refuse "$bin answers '$first' in $image, not '$expected'"
    echo "$bin answers '$first' in $image"
}

[ "$#" -ge 1 ] || usage
mode=$1
shift
case $mode in
    glibc-floor)
        [ "$#" -eq 2 ] || usage
        glibc_floor "$1" "$2"
        ;;
    static)
        [ "$#" -eq 1 ] || usage
        static "$1"
        ;;
    runs-in)
        [ "$#" -eq 3 ] || usage
        runs_in "$1" "$2" "$3"
        ;;
    *) usage ;;
esac
