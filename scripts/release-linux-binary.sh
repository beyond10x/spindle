#!/bin/sh
# Checks of a Linux release binary and of the release's image, run by the `build`, `image` and
# `image-manifest` jobs of .github/workflows/release.yml (story:release-linux-glibc-floor-and-musl,
# story:spindle-image-published). Each mode prints one line and exits 0, or names what it refused
# and exits 1.
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
#   release-linux-binary.sh containerfile <file>
#       the image's recipe holds only FROM (one, pinned by digest, no stage name), LABEL, COPY
#       (not --from; one --chmod whose mode sets no setuid, setgid or sticky bit), USER (one, numeric,
#       neither UID nor GID 0) and ENTRYPOINT ["/b10x-spindle"]: nothing runs while the image is
#       built, and no parser directive or continuation line (a backslash before trailing blanks too)
#   release-linux-binary.sh image-version <image> <first line>
#       the image's entrypoint answers --version with <first line>, with no network and a
#       read-only root; the image has no shell, so nothing but its entrypoint runs
#   release-linux-binary.sh image-user <image>
#       the image's configured user is a numeric UID other than 0 (and a GID other than 0)
#   release-linux-binary.sh image-answers <image> <specification> <environment>
#       `run` in the container on 0.0.0.0, with --trusted-caller-header and a fresh
#       --trusted-caller-secret, answers GET /__spindle/live on the port published on the host's
#       127.0.0.1 with 200 when the request carries the secret and 403 when it does not; tried
#       RELEASE_IMAGE_ATTEMPTS times (30), a second apart, until it answers 200
#   release-linux-binary.sh manifest-list <image@sha256:digest> [<amd64 digest> <arm64 digest>]
#       the digest resolves to a manifest list of exactly linux/amd64 and linux/arm64, each entry
#       naming a platform; given the two digests, the linux/amd64 entry names the first and the
#       linux/arm64 entry the second
#   release-linux-binary.sh images-line <file> <repository> <version>
#       the file holds one line, <repository>:<version>@sha256:<64 lowercase hex digits>, and
#       <version> is <major>.<minor>.<patch>
set -eu

usage() {
    echo "usage: $0 glibc-floor <binary> <floor> | static <binary> | runs-in <image@sha256:digest> <binary> <first line>" \
        "| containerfile <file> | image-version <image> <first line> | image-user <image>" \
        "| image-answers <image> <specification> <environment> | manifest-list <image@sha256:digest> [<amd64 digest> <arm64 digest>]" \
        "| images-line <file> <repository> <version>" >&2
    exit 2
}

refuse() {
    echo "release-linux-binary: refused: $*" >&2
    exit 1
}

binary() {
    [ -f "$1" ] || refuse "$1 is not a file"
}

# Whether $1 is 64 lowercase hex digits, the form of a sha256 digest.
sha256_hex() {
    [ "${#1}" -eq 64 ] || return 1
    case $1 in
        *[!0-9a-f]*) return 1 ;;
    esac
}

# Refuses an image reference that is not pinned by a sha256 digest.
pinned() {
    case $1 in
        *@sha256:*) ;;
        *) refuse "image $1 is not pinned by digest" ;;
    esac
    sha256_hex "${1##*@sha256:}" || refuse "image $1 is not pinned by a sha256 digest"
}

# Whether $1 is a user a container may run as here: a numeric UID, optionally `:` and a numeric
# GID, neither of them 0. A name is refused, since only the image's passwd file says what UID it is.
numeric_user() {
    case $1 in
        '' | *[!0-9:]* | *:*:* | :* | *:) return 1 ;;
    esac
    for id in "${1%%:*}" "${1#*:}"; do
        [ -n "$(printf '%s' "$id" | sed 's/^0*//')" ] || return 1
    done
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
    pinned "$image"
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

# Refuses a COPY whose files could carry a setuid, setgid or sticky bit: a root-owned setuid
# entrypoint runs as UID 0 whatever USER says. Each COPY sets the mode itself with one --chmod,
# since without it the file keeps the mode it has in the build context; an octal mode has no digit
# above the permission bits, and a symbolic one names only u, g, o, a, +, -, =, r, w, x, X and ",".
copy_mode() {
    file=$1
    line=$2
    argument=$3
    case $argument in
        *--chmod=*--chmod*) refuse "$file: COPY sets --chmod twice: $line" ;;
        *--chmod=*) ;;
        *) refuse "$file: COPY sets no --chmod, so the file keeps the build context's mode: $line" ;;
    esac
    mode=${argument#*--chmod=}
    mode=${mode%% *}
    case $mode in
        '') refuse "$file: COPY --chmod has no mode: $line" ;;
        *[!0-7]*)
            case $mode in
                *[!ugoa+=,rwxX-]*) refuse "$file: COPY --chmod=$mode names more than read, write and execute bits: $line" ;;
            esac
            ;;
        ????*)
            case $mode in
                ?????* | [1-7]*) refuse "$file: COPY --chmod=$mode sets a setuid, setgid or sticky bit: $line" ;;
            esac
            ;;
    esac
}

containerfile() {
    file=$1
    [ -f "$file" ] || refuse "$file is not a file"
    froms=0
    copies=0
    users=0
    entrypoints=0
    # Tabs read as spaces, so one space-separated reading covers both.
    lines=$(tr '\t' ' ' <"$file") || refuse "could not read $file"
    while IFS= read -r line; do
        # Trailing blanks dropped first: BuildKit reads a backslash followed by blanks as a
        # continuation too.
        line=${line%"${line##*[! ]}"}
        case $line in
            '') continue ;;
            *\\) refuse "$file: a continuation line: $line" ;;
            '#'*)
                # A parser directive (`# syntax=`, `# escape=`, `# check=`) picks the frontend or
                # the escape character; any other comment is skipped.
                comment=${line#\#}
                comment=${comment#"${comment%%[! ]*}"}
                key=${comment%%=*}
                key=${key%"${key##*[! ]}"}
                case $comment in
                    *=*)
                        case $key in
                            '' | *[!A-Za-z]*) ;;
                            *) refuse "$file: a parser directive: $line" ;;
                        esac
                        ;;
                esac
                continue
                ;;
        esac
        instruction=${line%% *}
        argument=${line#"$instruction"}
        argument=${argument#"${argument%%[! ]*}"}
        case $instruction in
            FROM)
                froms=$((froms + 1))
                case $argument in
                    '' | *' '*) refuse "$file: FROM takes one image and no stage name or flag: $line" ;;
                esac
                pinned "$argument"
                ;;
            COPY)
                copies=$((copies + 1))
                case $argument in
                    *--from*) refuse "$file: COPY from another image or stage: $line" ;;
                esac
                copy_mode "$file" "$line" "$argument"
                ;;
            USER)
                users=$((users + 1))
                numeric_user "$argument" || refuse "$file: USER is not a numeric UID and GID other than 0: $line"
                ;;
            ENTRYPOINT)
                entrypoints=$((entrypoints + 1))
                [ "$argument" = '["/b10x-spindle"]' ] || refuse "$file: ENTRYPOINT is not [\"/b10x-spindle\"]: $line"
                ;;
            LABEL) ;;
            *) refuse "$file: $instruction is none of FROM, LABEL, COPY, USER and ENTRYPOINT: $line" ;;
        esac
    done <<EOF
$lines
EOF
    [ "$froms" -eq 1 ] || refuse "$file has $froms FROM lines, not one"
    [ "$copies" -ge 1 ] || refuse "$file copies nothing into the image"
    [ "$users" -eq 1 ] || refuse "$file has $users USER lines, not one; without one the image runs as root"
    [ "$entrypoints" -eq 1 ] || refuse "$file has $entrypoints ENTRYPOINT lines, not one"
    echo "$file: one FROM pinned by digest, COPY only, a numeric user other than 0, ENTRYPOINT b10x-spindle"
}

image_version() {
    image=$1
    expected=$2
    answer=$(${DOCKER:-docker} run --rm --network none --read-only "$image" --version) ||
        refuse "the entrypoint of $image does not answer --version"
    first=$(printf '%s\n' "$answer" | head -n 1)
    [ "$first" = "$expected" ] || refuse "the entrypoint of $image answers '$first', not '$expected'"
    echo "the entrypoint of $image answers '$first'"
}

image_user() {
    image=$1
    user=$(${DOCKER:-docker} image inspect --format '{{.Config.User}}' "$image") ||
        refuse "docker could not inspect $image"
    numeric_user "$user" || refuse "$image runs as '$user', not a numeric UID and GID other than 0"
    echo "$image runs as $user"
}

# The container and the directory of its secret, removed on every exit of image-answers.
answers_cleanup() {
    if [ -n "${container:-}" ]; then
        ${DOCKER:-docker} rm --force "$container" >/dev/null 2>&1 || true
    fi
    if [ -n "${secrets:-}" ]; then
        rm -rf "$secrets"
    fi
}

image_answers() {
    image=$1
    spec=$2
    environment=$3
    [ -d "$spec" ] || refuse "$spec is not a directory"
    [ -f "$environment" ] || refuse "$environment is not a file"
    spec=$(cd "$spec" && pwd)
    environment=$(cd "$(dirname "$environment")" && pwd)/$(basename "$environment")
    attempts=${RELEASE_IMAGE_ATTEMPTS:-30}
    container=""
    secrets=""
    trap answers_cleanup EXIT
    secrets=$(mktemp -d) || refuse "mktemp could not make a directory for the secret"
    mkdir "$secrets/mount" || refuse "could not make $secrets/mount"
    secret=$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n') || refuse "could not read a secret from /dev/urandom"
    [ "${#secret}" -eq 48 ] || refuse "could not read a secret from /dev/urandom"
    # The container's user is not the runner's: the secret it reads is world-readable inside a
    # directory only this check knows; the header curl sends stays outside the mount.
    printf '%s\n' "$secret" >"$secrets/mount/secret"
    printf 'Spindle-Proxy-Secret: %s\n' "$secret" >"$secrets/header"
    chmod 755 "$secrets/mount"
    chmod 644 "$secrets/mount/secret"
    chmod 600 "$secrets/header"
    container=$(${DOCKER:-docker} run --detach --read-only --tmpfs /tmp --publish 127.0.0.1::8080 \
        --volume "$spec:/spindle/spec:ro" --volume "$environment:/spindle/environment.toml:ro" \
        --volume "$secrets/mount:/spindle/secret:ro" \
        "$image" run /spindle/spec --environment /spindle/environment.toml \
        --address 0.0.0.0 --port 8080 \
        --trusted-caller-header Spindle-Caller --trusted-caller-secret /spindle/secret/secret) ||
        refuse "$image did not start run on 0.0.0.0"
    container=$(printf '%s\n' "$container" | tail -n 1)
    [ -n "$container" ] || refuse "docker named no container for $image"
    published=$(${DOCKER:-docker} port "$container" 8080/tcp | head -n 1) ||
        refuse "docker could not say where port 8080 of $image is published"
    port=${published##*:}
    case $published in
        *:*) ;;
        *) refuse "docker published port 8080 of $image as '$published'" ;;
    esac
    case $port in
        '' | *[!0-9]*) refuse "docker published port 8080 of $image as '$published'" ;;
    esac
    url=http://127.0.0.1:$port/__spindle/live
    with=""
    tried=0
    while [ "$tried" -lt "$attempts" ]; do
        tried=$((tried + 1))
        with=$(curl --silent --max-time 5 --output /dev/null --write-out '%{http_code}' \
            --header "@$secrets/header" "$url") || true
        [ "$with" != 200 ] || break
        [ "$tried" -ge "$attempts" ] || sleep 1
    done
    [ "$with" = 200 ] ||
        refuse "$url answered '$with' with the secret after $tried attempts, not 200; the container's log: $(${DOCKER:-docker} logs "$container" 2>&1 | tail -n 20)"
    without=$(curl --silent --max-time 5 --output /dev/null --write-out '%{http_code}' "$url") || true
    [ "$without" = 403 ] || refuse "$url answered '$without' without the secret, not 403"
    echo "$image: run on 0.0.0.0 answers $url 200 with the proxy secret and 403 without it"
}

manifest_list() {
    ref=$1
    pinned "$ref"
    raw=$(${DOCKER:-docker} buildx imagetools inspect --raw "$ref") || refuse "$ref resolves to nothing"
    compact=$(printf '%s' "$raw" | tr -d ' \t\r\n')
    case $compact in
        *'"layers":'*) refuse "$ref is one image's manifest, not a manifest list" ;;
    esac
    case $compact in
        *'"manifests":['*) ;;
        *) refuse "$ref is not a manifest list" ;;
    esac
    # Every entry of the list names its manifest's digest, whatever the algorithm, and must name a
    # platform; a list whose entries are not exactly one linux/amd64 and one linux/arm64 is
    # refused, an entry without a platform included.
    entries=$(printf '%s\n' "$compact" | grep -o '"digest":"[^"]*"' | grep -c '') || true
    platforms=$(printf '%s\n' "$compact" | grep -o '"platform":{[^}]*}' | while IFS= read -r block; do
        os=$(printf '%s\n' "$block" | sed -n 's/.*[{,]"os":"\([^"]*\)".*/\1/p')
        arch=$(printf '%s\n' "$block" | sed -n 's/.*[{,]"architecture":"\([^"]*\)".*/\1/p')
        printf '%s/%s\n' "$os" "$arch"
    done | sort)
    named=$(printf '%s\n' "$compact" | grep -o '"platform":{' | grep -c '') || true
    found=$(printf '%s' "$platforms" | tr '\n' ' ')
    [ "$entries" -eq "$named" ] || refuse "$ref lists $entries manifests and names $named platforms"
    [ "$entries" -eq 2 ] && [ "$found" = "linux/amd64 linux/arm64" ] ||
        refuse "$ref lists $entries manifests for '$found', not one for linux/amd64 and one for linux/arm64"
    if [ "$#" -eq 3 ]; then
        for digest in "$2" "$3"; do
            case $digest in
                sha256:*) sha256_hex "${digest#sha256:}" || refuse "'$digest' is not a sha256 digest" ;;
                *) refuse "'$digest' is not a sha256 digest" ;;
            esac
        done
        # One entry per line: entries are the only objects in an array of the list, so `},{`
        # separates them and nothing else.
        held=$(printf '%s\n' "$compact" | sed 's/},{/}\n{/g' | while IFS= read -r entry; do
            digest=$(printf '%s\n' "$entry" | sed -n 's/.*"digest":"\([^"]*\)".*/\1/p')
            arch=$(printf '%s\n' "$entry" | sed -n 's/.*"platform":{[^}]*"architecture":"\([^"]*\)".*/\1/p')
            [ -z "$digest" ] || printf '%s=%s\n' "$arch" "$digest"
        done | sort | tr '\n' ' ')
        [ "$held" = "amd64=$2 arm64=$3 " ] ||
            refuse "$ref holds '$held', not linux/amd64 $2 and linux/arm64 $3"
        echo "$ref: a manifest list of linux/amd64 $2 and linux/arm64 $3"
        return
    fi
    echo "$ref: a manifest list of linux/amd64 and linux/arm64"
}

images_line() {
    file=$1
    repository=$2
    version=$3
    # A release version is three dot-separated numbers; an empty one names no tag.
    case $version in
        *[!0-9.]* | .* | *. | *..* | *.*.*.*) refuse "the version '$version' is not <major>.<minor>.<patch>" ;;
        *.*.*) ;;
        *) refuse "the version '$version' is not <major>.<minor>.<patch>" ;;
    esac
    [ -f "$file" ] || refuse "$file is not a file"
    [ "$(grep -c '' "$file")" -eq 1 ] || refuse "$file does not hold exactly one line"
    line=$(cat "$file")
    case $line in
        "$repository:$version@sha256:"*) ;;
        *) refuse "$file holds '$line', not $repository:$version@sha256:<digest>" ;;
    esac
    sha256_hex "${line#"$repository:$version@sha256:"}" ||
        refuse "$file holds '$line', whose digest is not 64 lowercase hex digits"
    echo "$file: $line"
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
    containerfile)
        [ "$#" -eq 1 ] || usage
        containerfile "$1"
        ;;
    image-version)
        [ "$#" -eq 2 ] || usage
        image_version "$1" "$2"
        ;;
    image-user)
        [ "$#" -eq 1 ] || usage
        image_user "$1"
        ;;
    image-answers)
        [ "$#" -eq 3 ] || usage
        image_answers "$1" "$2" "$3"
        ;;
    manifest-list)
        [ "$#" -eq 1 ] || [ "$#" -eq 3 ] || usage
        manifest_list "$@"
        ;;
    images-line)
        [ "$#" -eq 3 ] || usage
        images_line "$1" "$2" "$3"
        ;;
    *) usage ;;
esac
