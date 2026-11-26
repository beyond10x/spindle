#!/bin/sh
# Checks of a Linux release binary, run by the `build` job of .github/workflows/release.yml
# (story:release-linux-glibc-floor-and-musl). Each mode prints one line and exits 0, or names
# what it refused and exits 1.
#
#   release-linux-binary.sh glibc-floor <binary> <floor>
#       every GLIBC_ symbol version (objdump -T) and version reference (objdump -p) of the binary
#       dates from glibc <floor> or older; GLIBC_ABI_DT_RELR counts as 2.36, any other name
#       without a number is refused
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

# The glibc release that first defines a GLIBC_ version name the loader checks: a numbered
# GLIBC_2.x is its own release; an ABI tag is dated here. A name this table cannot date is refused,
# since no floor can be shown for it.
glibc_release() {
    case $1 in
        GLIBC_[0-9]*) printf '%s\n' "${1#GLIBC_}" ;;
        GLIBC_ABI_DT_RELR) echo 2.36 ;;
        *) return 1 ;;
    esac
}

glibc_floor() {
    bin=$1
    floor=$2
    binary "$bin"
    # The symbol versions (objdump -T) and the version references (objdump -p): ld.so checks every
    # reference, and an ABI tag such as GLIBC_ABI_DT_RELR has no symbol, so it shows only in -p.
    dynsym=$(objdump -T "$bin") || refuse "objdump -T could not read $bin"
    verneed=$(objdump -p "$bin") || refuse "objdump -p could not read $bin"
    names=$(printf '%s\n%s\n' "$dynsym" "$verneed" | grep -o 'GLIBC_[A-Za-z0-9_.]*' | sort -u) || true
    [ -n "$names" ] || refuse "$bin names no GLIBC_ version; it is no glibc binary"
    above=""
    undated=""
    versions=""
    for name in $names; do
        if ! version=$(glibc_release "$name"); then
            undated="$undated $name"
            continue
        fi
        versions="$versions$version
"
        if [ "$version" != "$floor" ] &&
            [ "$(printf '%s\n%s\n' "$floor" "$version" | sort -V | tail -n 1)" = "$version" ]; then
            case $name in
                GLIBC_[0-9]*) above="$above $name" ;;
                *) above="$above $name (glibc $version)" ;;
            esac
        fi
    done
    [ -z "$undated" ] || refuse "$bin needs${undated}, which no glibc release is known for here"
    [ -z "$above" ] || refuse "$bin needs${above}, above the glibc $floor floor"
    newest=$(printf '%s' "$versions" | sort -u -V | tail -n 1)
    echo "$bin: newest GLIBC_ symbol version $newest, floor $floor"
}

static() {
    bin=$1
    binary "$bin"
    described=$(file --brief "$bin") || refuse "file could not read $bin"
    case $described in
        *"statically linked"* | *"static-pie linked"*) ;;
        *) refuse "$bin is not statically linked: $described" ;;
    esac
    # Read readelf's output before searching it: a readelf that fails or is missing must refuse,
    # not leave the INTERP and NEEDED searches nothing to find.
    phdrs=$(readelf --program-headers --wide "$bin") || refuse "readelf --program-headers could not read $bin"
    [ -n "$phdrs" ] || refuse "readelf --program-headers printed nothing for $bin"
    dynamic=$(readelf --dynamic --wide "$bin") || refuse "readelf --dynamic could not read $bin"
    if printf '%s\n' "$phdrs" | grep -q INTERP; then
        refuse "$bin names a program interpreter"
    fi
    needed=$(printf '%s\n' "$dynamic" | grep '(NEEDED)') || true
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
