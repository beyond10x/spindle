#!/bin/sh
# Adversarial cases for scripts/release-linux-binary.sh, the Containerfile and the target lists of
# .github/workflows/release.yml (story:release-linux-glibc-floor-and-musl,
# story:spindle-image-published). POSIX sh; run as
# `sh scripts/release-linux-binary.adversary-test.sh`. The checked tools (objdump, file, readelf,
# docker, curl) are stubs that print recorded output, so no case builds, pulls, pushes or runs an
# image or reaches a registry; the stubs sit in a PATH of their own, so a case can also take a
# tool away.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
subject=${RELEASE_LINUX_BINARY:-$root/scripts/release-linux-binary.sh}
workflow=${RELEASE_WORKFLOW:-$root/.github/workflows/release.yml}
containerfile=${RELEASE_CONTAINERFILE:-$root/Containerfile}
work=$root/target/release-linux-binary-adversary
rm -rf "$work"
mkdir -p "$work/rec" "$work/tools" "$work/tmp"
# The image check makes its secret under TMPDIR; keep it in this run's directory, and let it give up
# after two attempts with a sleep that returns at once.
TMPDIR=$work/tmp
RELEASE_IMAGE_ATTEMPTS=2
export TMPDIR RELEASE_IMAGE_ATTEMPTS

failures=0
cases=0
shell=$(command -v sh)

fail() {
    echo "release-linux-binary-adversary: FAIL: $*" >&2
    failures=$((failures + 1))
}

# The plain tools the subject and the stubs call, without any of the checked ones.
for tool in grep sed sort tail head dirname basename cat cp tr od mktemp mkdir chmod rm; do
    ln -s "$(command -v "$tool")" "$work/tools/$tool"
done

stub() {
    name=$1
    body=$2
    printf '#!%s\n%s\n' "$shell" "$body" >"$work/tools/$name"
    chmod 755 "$work/tools/$name"
}

stub objdump "case \$1 in -T) cat '$work/rec/objdump-T' ;; -p) cat '$work/rec/objdump-p' ;; *) exit 2 ;; esac"
stub file "cat '$work/rec/file'"
stub readelf "case \$1 in --program-headers) cat '$work/rec/phdrs' ;; --dynamic) cat '$work/rec/dynamic' ;; *) exit 2 ;; esac"
# docker: a foreground `run` records its arguments in docker-args and answers docker-out with the
# status in docker-status; every other call is appended to docker-log and answered from its own
# recording. A detached `run` keeps a copy of the secret mounted at /spindle/secret, so a case can
# compare it with the header curl was given.
printf '#!%s\nrec=%s\n' "$shell" "$work/rec" >"$work/tools/docker"
cat >>"$work/tools/docker" <<'STUB'
printf '%s\n' "$*" >>"$rec/docker-log"
case "$1 $2" in
    "run --detach")
        for arg in "$@"; do
            case $arg in *:/spindle/secret:ro) cp "${arg%%:*}/secret" "$rec/secret-seen" ;; esac
        done
        cat "$rec/run-id"
        exit "$(cat "$rec/run-status")"
        ;;
    "port "*) cat "$rec/port-out"; exit "$(cat "$rec/port-status")" ;;
    "rm "*) printf '%s\n' "$*" >>"$rec/removed"; exit 0 ;;
    "logs "*) echo "recorded container log"; exit 0 ;;
    "image inspect") cat "$rec/user-out"; exit "$(cat "$rec/user-status")" ;;
    "buildx imagetools") cat "$rec/raw"; exit "$(cat "$rec/raw-status")" ;;
esac
printf '%s\n' "$*" >"$rec/docker-args"
cat "$rec/docker-out"
exit "$(cat "$rec/docker-status")"
STUB
chmod 755 "$work/tools/docker"

# curl: answers curl-with when given a header file (`--header @<file>`, kept as header-seen) and
# curl-without otherwise, with the status in curl-status.
printf '#!%s\nrec=%s\n' "$shell" "$work/rec" >"$work/tools/curl"
cat >>"$work/tools/curl" <<'STUB'
printf '%s\n' "$*" >>"$rec/curl-log"
answer=$rec/curl-without
for arg in "$@"; do
    case $arg in @*) cp "${arg#@}" "$rec/header-seen"; answer=$rec/curl-with ;; esac
done
cat "$answer"
exit "$(cat "$rec/curl-status")"
STUB
chmod 755 "$work/tools/curl"
stub sleep ":"

bin=$work/b10x-spindle
printf 'not a real binary; the stubs answer for it\n' >"$bin"

# Runs the subject with only the stub tools on PATH; prints its exit status.
run() {
    set +e
    PATH=$work/tools "$shell" "$subject" "$@" >"$work/out" 2>"$work/err"
    status=$?
    set -e
    echo "$status"
}

expect() {
    want=$1
    label=$2
    shift 2
    cases=$((cases + 1))
    got=$(run "$@")
    case $want in
        pass) [ "$got" -eq 0 ] || fail "$label: refused ($got): $(cat "$work/err")" ;;
        refuse)
            # A refusal exits 1 and says so; exit 2 is the usage line of a mode the script lacks.
            [ "$got" -eq 1 ] && grep -q '^release-linux-binary: refused: ' "$work/err" ||
                fail "$label: not refused (exit $got): $(cat "$work/out" "$work/err")"
            ;;
    esac
}

dynsym() {
    {
        printf '%s:     file format elf64-x86-64\n\nDYNAMIC SYMBOL TABLE:\n' "$bin"
        for v in "$@"; do
            printf '0000000000000000      DF *UND*\t0000000000000000 (GLIBC_%s) sym_%s\n' "$v" "$v"
        done
    } >"$work/rec/objdump-T"
}

verneed() {
    {
        printf 'Version References:\n  required from libc.so.6:\n'
        for v in "$@"; do
            printf '    0x00000000 0x00 00 %s\n' "$v"
        done
    } >"$work/rec/objdump-p"
}

# --- glibc-floor -----------------------------------------------------------------------------

dynsym 2.2.5 2.3.4 2.14 2.17
verneed GLIBC_2.2.5 GLIBC_2.3.4 GLIBC_2.14 GLIBC_2.17
expect pass "glibc-floor: newest GLIBC_2.17 is on the 2.17 floor" glibc-floor "$bin" 2.17
grep -Fq "newest GLIBC_ symbol version 2.17, floor 2.17" "$work/out" ||
    fail "glibc-floor: the pass line does not name 2.17 as the newest version: $(cat "$work/out")"
cases=$((cases + 1))

dynsym 2.2.5 2.18
verneed GLIBC_2.2.5 GLIBC_2.18
expect refuse "glibc-floor: GLIBC_2.18 is one above the 2.17 floor" glibc-floor "$bin" 2.17
grep -Fq "GLIBC_2.18" "$work/err" || fail "glibc-floor: the refusal does not name GLIBC_2.18: $(cat "$work/err")"
cases=$((cases + 1))

dynsym 2.2.5 2.34 2.39
verneed GLIBC_2.2.5 GLIBC_2.34 GLIBC_2.39
expect refuse "glibc-floor: GLIBC_2.39, the version 0.21.0 needed" glibc-floor "$bin" 2.17

: >"$work/rec/objdump-T"
expect refuse "glibc-floor: objdump prints nothing" glibc-floor "$bin" 2.17

mv "$work/tools/objdump" "$work/objdump.away"
dynsym 2.2.5
expect refuse "glibc-floor: no objdump on PATH" glibc-floor "$bin" 2.17
mv "$work/objdump.away" "$work/tools/objdump"

expect refuse "glibc-floor: no such binary" glibc-floor "$work/missing" 2.17

# A binary linked with packed relative relocations (-z pack-relative-relocs) needs
# GLIBC_ABI_DT_RELR, which only glibc 2.36 and newer define. It is a version reference with no
# symbol, so objdump -T does not show it; ld.so still refuses the binary on an older glibc:
#   /release/ls: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_ABI_DT_RELR' not found
# (ubuntu 22.04, glibc 2.35). The floor check must refuse it.
dynsym 2.2.5 2.3.4 2.17
verneed GLIBC_ABI_DT_RELR GLIBC_2.2.5 GLIBC_2.3.4 GLIBC_2.17
expect refuse "glibc-floor: a binary that needs GLIBC_ABI_DT_RELR (glibc 2.36) passes the 2.17 floor" \
    glibc-floor "$bin" 2.17

# The same with a real binary of this host, when one needs GLIBC_ABI_DT_RELR: at the floor of its
# newest numbered GLIBC_ version the check must still refuse it.
for host_bin in /usr/bin/true /usr/bin/cat /usr/bin/ls; do
    command -v objdump >/dev/null 2>&1 || break
    [ -f "$host_bin" ] || continue
    objdump -p "$host_bin" 2>/dev/null | grep -q GLIBC_ABI_DT_RELR || continue
    newest=$(objdump -T "$host_bin" 2>/dev/null | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/^GLIBC_//' | sort -u -V | tail -n 1)
    [ -n "$newest" ] || continue
    cases=$((cases + 1))
    if sh "$subject" glibc-floor "$host_bin" "$newest" >/dev/null 2>&1; then
        fail "glibc-floor: $host_bin needs GLIBC_ABI_DT_RELR (glibc 2.36) and passes the $newest floor"
    fi
    break
done

# --- static ----------------------------------------------------------------------------------

static_ok() {
    printf '%s\n' "$1" >"$work/rec/file"
    printf 'Program Headers:\n  Type           Offset   VirtAddr\n  LOAD           0x000000 0x0000000000400000\n  GNU_STACK      0x000000 0x0000000000000000\n' >"$work/rec/phdrs"
    printf '\nThere is no dynamic section in this file.\n' >"$work/rec/dynamic"
}

static_ok "ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked, stripped"
expect pass "static: statically linked, no INTERP, no NEEDED" static "$bin"

static_ok "ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), static-pie linked, stripped"
printf 'Dynamic section at offset 0x12bbe8 contains 28 entries:\n 0x000000000000000c (INIT)               0x1000\n' >"$work/rec/dynamic"
expect pass "static: static-pie linked with a dynamic section and no NEEDED" static "$bin"

static_ok "ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), dynamically linked, interpreter /lib/ld-musl-x86_64.so.1, stripped"
expect refuse "static: a dynamic musl binary" static "$bin"

static_ok "ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked, stripped"
printf 'Program Headers:\n  INTERP         0x000318 0x0000000000000318\n      [Requesting program interpreter: /lib/ld-musl-x86_64.so.1]\n' >"$work/rec/phdrs"
expect refuse "static: file says static, readelf shows INTERP" static "$bin"

static_ok "ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked, stripped"
printf ' 0x0000000000000001 (NEEDED)             Shared library: [libc.musl-x86_64.so.1]\n' >"$work/rec/dynamic"
expect refuse "static: file says static, readelf shows NEEDED" static "$bin"

static_ok "ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), dynamically linked, interpreter /lib64/ld-linux-x86-64.so.2"
mv "$work/tools/file" "$work/file.away"
expect refuse "static: no file on PATH" static "$bin"
mv "$work/file.away" "$work/tools/file"

# Without readelf the INTERP and NEEDED checks read nothing and the row passes on file(1) alone.
static_ok "ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked, stripped"
printf 'Program Headers:\n  INTERP         0x000318 0x0000000000000318\n' >"$work/rec/phdrs"
printf ' 0x0000000000000001 (NEEDED)             Shared library: [libc.so.6]\n' >"$work/rec/dynamic"
mv "$work/tools/readelf" "$work/readelf.away"
expect refuse "static: no readelf on PATH (the INTERP and NEEDED checks read nothing)" static "$bin"
mv "$work/readelf.away" "$work/tools/readelf"

# --- runs-in ---------------------------------------------------------------------------------

digest=f59abe656b692d901e184be3445e084786602108599d2a51d229c0f5a48c71bf
image="alpine:3.24.4@sha256:$digest"

printf 'b10x-spindle 0.22.0\n' >"$work/rec/docker-out"
echo 0 >"$work/rec/docker-status"
rm -f "$work/rec/docker-args"
expect pass "runs-in: the binary answers the expected line" runs-in "$image" "$bin" "b10x-spindle 0.22.0"
grep -Fq -- "--network none" "$work/rec/docker-args" ||
    fail "runs-in: docker ran without --network none: $(cat "$work/rec/docker-args")"
grep -Fq "$image" "$work/rec/docker-args" ||
    fail "runs-in: docker did not run the pinned image: $(cat "$work/rec/docker-args")"
cases=$((cases + 2))

expect refuse "runs-in: the binary answers another version" runs-in "$image" "$bin" "b10x-spindle 0.21.0"

echo 1 >"$work/rec/docker-status"
expect refuse "runs-in: the container exits non-zero" runs-in "$image" "$bin" "b10x-spindle 0.22.0"
echo 0 >"$work/rec/docker-status"

: >"$work/rec/docker-out"
expect refuse "runs-in: the binary prints nothing" runs-in "$image" "$bin" "b10x-spindle 0.22.0"
printf 'b10x-spindle 0.22.0\n' >"$work/rec/docker-out"

for unpinned in alpine:3.24.4 "alpine@sha256:abc" "alpine@sha256:$(printf '%s' "$digest" | tr 'a-f' 'A-F')" \
    "alpine@sha256:${digest}0" "alpine@sha512:$digest"; do
    rm -f "$work/rec/docker-args"
    expect refuse "runs-in: image $unpinned" runs-in "$unpinned" "$bin" "b10x-spindle 0.22.0"
    [ ! -e "$work/rec/docker-args" ] || fail "runs-in: docker ran the image $unpinned that is not pinned"
done

mv "$work/tools/docker" "$work/docker.away"
expect refuse "runs-in: no docker on PATH" runs-in "$image" "$bin" "b10x-spindle 0.22.0"
mv "$work/docker.away" "$work/tools/docker"

# --- containerfile ---------------------------------------------------------------------------
# adversary_containerfile_is_pinned_nonroot_copy_only: the image's recipe pins its base by digest,
# copies the binary in, runs as a numeric user other than 0 with b10x-spindle as the entrypoint,
# and runs nothing while it is built.

base="gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f"
recipe() {
    printf '%s\n' "$@" >"$work/Containerfile"
}
good_recipe() {
    recipe "FROM $base" 'LABEL org.opencontainers.image.source="https://github.com/beyond10x/spindle"' \
        'COPY --chmod=0555 b10x-spindle /b10x-spindle' 'USER 65532:65532' 'ENTRYPOINT ["/b10x-spindle"]'
}

expect pass "adversary_containerfile_is_pinned_nonroot_copy_only: the repository's Containerfile" \
    containerfile "$containerfile"
good_recipe
expect pass "adversary_containerfile_is_pinned_nonroot_copy_only: a pinned, numeric, copy-only recipe" \
    containerfile "$work/Containerfile"

recipe "FROM gcr.io/distroless/cc-debian12:nonroot" 'COPY b10x-spindle /b10x-spindle' 'USER 65532' 'ENTRYPOINT ["/b10x-spindle"]'
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: FROM by tag" containerfile "$work/Containerfile"
recipe "FROM gcr.io/distroless/cc-debian12@sha256:abc" 'COPY b10x-spindle /b10x-spindle' 'USER 65532' 'ENTRYPOINT ["/b10x-spindle"]'
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: FROM by a short digest" containerfile "$work/Containerfile"
recipe "FROM $base AS base" 'COPY b10x-spindle /b10x-spindle' 'USER 65532' 'ENTRYPOINT ["/b10x-spindle"]'
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: a named stage" containerfile "$work/Containerfile"
recipe "FROM $base" "FROM $base" 'COPY b10x-spindle /b10x-spindle' 'USER 65532' 'ENTRYPOINT ["/b10x-spindle"]'
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: two FROM" containerfile "$work/Containerfile"
recipe 'COPY b10x-spindle /b10x-spindle' 'USER 65532' 'ENTRYPOINT ["/b10x-spindle"]'
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: no FROM" containerfile "$work/Containerfile"
for user in 0 0:0 00 root nonroot 65532:0 '' 65532:nonroot 65532: :65532; do
    recipe "FROM $base" 'COPY b10x-spindle /b10x-spindle' "USER $user" 'ENTRYPOINT ["/b10x-spindle"]'
    expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: USER '$user'" containerfile "$work/Containerfile"
done
recipe "FROM $base" 'COPY b10x-spindle /b10x-spindle' 'ENTRYPOINT ["/b10x-spindle"]'
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: no USER, so root" containerfile "$work/Containerfile"
recipe "FROM $base" 'COPY b10x-spindle /b10x-spindle' 'USER 65532' 'USER 0' 'ENTRYPOINT ["/b10x-spindle"]'
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: a second USER 0" containerfile "$work/Containerfile"
for instruction in 'RUN ["/b10x-spindle", "--version"]' 'RUN true' 'run true' 'ADD https://example.org/x /x' \
    'COPY --from=busybox /bin/sh /bin/sh' 'ARG BASE' 'ONBUILD RUN true' 'SHELL ["/bin/sh", "-c"]' \
    '  RUN true' 'LABEL a=b \' '# syntax=docker/dockerfile:1' '#escape=`'; do
    good_recipe
    printf '%s\n' "$instruction" >>"$work/Containerfile"
    expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: $instruction" containerfile "$work/Containerfile"
done
for entrypoint in 'ENTRYPOINT ["/bin/sh"]' 'ENTRYPOINT /b10x-spindle' 'ENTRYPOINT ["/b10x-spindle", "run"]'; do
    recipe "FROM $base" 'COPY b10x-spindle /b10x-spindle' 'USER 65532' "$entrypoint"
    expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: $entrypoint" containerfile "$work/Containerfile"
done
recipe "FROM $base" 'COPY b10x-spindle /b10x-spindle' 'USER 65532'
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: no ENTRYPOINT" containerfile "$work/Containerfile"
recipe "FROM $base" 'USER 65532' 'ENTRYPOINT ["/b10x-spindle"]'
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: nothing copied" containerfile "$work/Containerfile"
expect refuse "adversary_containerfile_is_pinned_nonroot_copy_only: no such file" containerfile "$work/missing"

# --- image-version ---------------------------------------------------------------------------
# adversary_image_version_refuses_another_version: the image's entrypoint answers --version with
# the release's version, with no network and a read-only root.

checked=spindle-check:linux-arm64
printf 'b10x-spindle 0.26.0\n' >"$work/rec/docker-out"
echo 0 >"$work/rec/docker-status"
rm -f "$work/rec/docker-args"
expect pass "adversary_image_version_refuses_another_version: the image answers the release's version" \
    image-version "$checked" "b10x-spindle 0.26.0"
cases=$((cases + 1))
[ "$(cat "$work/rec/docker-args" 2>/dev/null)" = "run --rm --network none --read-only $checked --version" ] ||
    fail "image-version: docker did not run the image's own entrypoint with --version, no network and a read-only root: $(cat "$work/rec/docker-args" 2>/dev/null)"
for other in "b10x-spindle 0.25.0" "b10x-spindle 0.26.0-rc.1" "b10x-spindle" "0.26.0"; do
    expect refuse "adversary_image_version_refuses_another_version: the image answers, the release wants '$other'" \
        image-version "$checked" "$other"
done
printf 'b10x-spindle 0.25.0\nb10x-spindle 0.26.0\n' >"$work/rec/docker-out"
expect refuse "adversary_image_version_refuses_another_version: the release's version on the second line only" \
    image-version "$checked" "b10x-spindle 0.26.0"
: >"$work/rec/docker-out"
expect refuse "adversary_image_version_refuses_another_version: the image prints nothing" \
    image-version "$checked" "b10x-spindle 0.26.0"
printf 'b10x-spindle 0.26.0\n' >"$work/rec/docker-out"
echo 127 >"$work/rec/docker-status"
expect refuse "adversary_image_version_refuses_another_version: the entrypoint does not run" \
    image-version "$checked" "b10x-spindle 0.26.0"
echo 0 >"$work/rec/docker-status"
mv "$work/tools/docker" "$work/docker.away"
expect refuse "adversary_image_version_refuses_another_version: no docker on PATH" \
    image-version "$checked" "b10x-spindle 0.26.0"
mv "$work/docker.away" "$work/tools/docker"

# --- image-user ------------------------------------------------------------------------------
# adversary_image_user_refuses_root: the image's configured user is a numeric UID other than 0.

echo 0 >"$work/rec/user-status"
for user in 65532 65532:65532 1000; do
    printf '%s\n' "$user" >"$work/rec/user-out"
    expect pass "adversary_image_user_refuses_root: the image runs as '$user'" image-user "$checked"
done
cases=$((cases + 1))
grep -Fqx "image inspect --format {{.Config.User}} $checked" "$work/rec/docker-log" ||
    fail "image-user: docker did not read the image's configured user: $(cat "$work/rec/docker-log")"
for user in '' 0 0:0 000 root nonroot 0:65532 65532:0 65532:root 'root:65532' ' 65532' -1; do
    printf '%s\n' "$user" >"$work/rec/user-out"
    expect refuse "adversary_image_user_refuses_root: the image runs as '$user'" image-user "$checked"
done
printf '65532\n' >"$work/rec/user-out"
echo 1 >"$work/rec/user-status"
expect refuse "adversary_image_user_refuses_root: docker cannot inspect the image" image-user "$checked"
echo 0 >"$work/rec/user-status"

# --- image-answers ---------------------------------------------------------------------------
# adversary_image_answers_off_container: `run` in the container listens on 0.0.0.0 with a trusted
# caller header and secret, and a request sent to the published port from outside the container
# is answered 200 with the secret and 403 without it.

spec=$work/spec
mkdir -p "$spec"
environment=$work/laptop.toml
printf 'format = "spindle-environment/1"\n' >"$environment"
answers_ok() {
    printf 'c0ffee\n' >"$work/rec/run-id"
    echo 0 >"$work/rec/run-status"
    printf '127.0.0.1:49153\n' >"$work/rec/port-out"
    echo 0 >"$work/rec/port-status"
    printf '200' >"$work/rec/curl-with"
    printf '403' >"$work/rec/curl-without"
    echo 0 >"$work/rec/curl-status"
    rm -f "$work/rec/docker-log" "$work/rec/curl-log" "$work/rec/removed" "$work/rec/secret-seen" "$work/rec/header-seen"
}

answers_ok
expect pass "adversary_image_answers_off_container: 200 with the secret, 403 without" \
    image-answers "$checked" "$spec" "$environment"
started=$(grep '^run --detach' "$work/rec/docker-log" || true)
for want in "--publish 127.0.0.1::8080" "$checked run /spindle/spec --environment /spindle/environment.toml" \
    "--address 0.0.0.0 --port 8080" "--trusted-caller-header Spindle-Caller" \
    "--trusted-caller-secret /spindle/secret/secret" "--volume $spec:/spindle/spec:ro" \
    "--volume $environment:/spindle/environment.toml:ro" "--read-only"; do
    cases=$((cases + 1))
    case $started in
        *"$want"*) ;;
        *) fail "image-answers: the container was not started with '$want': $started" ;;
    esac
done
cases=$((cases + 1))
[ -s "$work/rec/secret-seen" ] && [ -s "$work/rec/header-seen" ] &&
    [ "$(cat "$work/rec/header-seen")" = "Spindle-Proxy-Secret: $(cat "$work/rec/secret-seen")" ] ||
    fail "image-answers: the request did not carry the secret the container was given"
cases=$((cases + 1))
grep -Fq "http://127.0.0.1:49153/__spindle/live" "$work/rec/curl-log" ||
    fail "image-answers: the request did not go to the published port: $(cat "$work/rec/curl-log")"
cases=$((cases + 1))
grep -Fqx "rm --force c0ffee" "$work/rec/removed" 2>/dev/null ||
    fail "image-answers: the container was not removed"
cases=$((cases + 1))
[ -z "$(ls "$work/tmp")" ] || fail "image-answers: the secret's directory was left behind: $(ls "$work/tmp")"

answers_ok
printf '403' >"$work/rec/curl-with"
expect refuse "adversary_image_answers_off_container: refused with the secret" \
    image-answers "$checked" "$spec" "$environment"
cases=$((cases + 1))
grep -Fqx "rm --force c0ffee" "$work/rec/removed" 2>/dev/null ||
    fail "image-answers: the container of a refused check was not removed"
cases=$((cases + 1))
[ -z "$(ls "$work/tmp")" ] || fail "image-answers: a refused check left the secret's directory behind"

answers_ok
printf '200' >"$work/rec/curl-without"
expect refuse "adversary_image_answers_off_container: answered without the secret" \
    image-answers "$checked" "$spec" "$environment"

answers_ok
printf '000' >"$work/rec/curl-with"
printf '000' >"$work/rec/curl-without"
echo 7 >"$work/rec/curl-status"
expect refuse "adversary_image_answers_off_container: nothing listens on the published port" \
    image-answers "$checked" "$spec" "$environment"
cases=$((cases + 1))
[ "$(grep -c '' "$work/rec/curl-log")" -eq "$RELEASE_IMAGE_ATTEMPTS" ] ||
    fail "image-answers: tried $(grep -c '' "$work/rec/curl-log") times, not $RELEASE_IMAGE_ATTEMPTS"

answers_ok
echo 125 >"$work/rec/run-status"
expect refuse "adversary_image_answers_off_container: the container does not start" \
    image-answers "$checked" "$spec" "$environment"

for published in "" "0.0.0.0" "127.0.0.1:http"; do
    answers_ok
    printf '%s\n' "$published" >"$work/rec/port-out"
    expect refuse "adversary_image_answers_off_container: docker port answers '$published'" \
        image-answers "$checked" "$spec" "$environment"
done

answers_ok
expect refuse "adversary_image_answers_off_container: no such specification" \
    image-answers "$checked" "$work/missing" "$environment"
expect refuse "adversary_image_answers_off_container: no such environment" \
    image-answers "$checked" "$spec" "$work/missing.toml"

answers_ok
mv "$work/tools/curl" "$work/curl.away"
expect refuse "adversary_image_answers_off_container: no curl on PATH" \
    image-answers "$checked" "$spec" "$environment"
mv "$work/curl.away" "$work/tools/curl"

# --- manifest-list ---------------------------------------------------------------------------
# adversary_manifest_list_needs_both_architectures: the digest in IMAGES resolves to a manifest
# list holding linux/amd64 and linux/arm64, nothing else.

listed=ghcr.io/beyond10x/spindle:0.26.0@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
entry() {
    printf '    {\n      "mediaType": "application/vnd.oci.image.manifest.v1+json",\n      "digest": "sha256:%s",\n      "size": 1000,\n      "platform": {\n        "os": "%s",\n        "architecture": "%s"%s\n      }\n    }' \
        "$(printf '%064d' "$1")" "$2" "$3" "$4"
}
index() {
    media=$1
    shift
    {
        printf '{\n  "schemaVersion": 2,\n  "mediaType": "%s",\n  "manifests": [\n' "$media"
        separator=""
        for platform in "$@"; do
            printf '%s' "$separator"
            n=$((${n:-0} + 1))
            os=${platform%%/*}
            rest=${platform#*/}
            arch=${rest%%/*}
            variant=""
            [ "$rest" = "$arch" ] || variant=",
        \"variant\": \"${rest#*/}\""
            entry "$n" "$os" "$arch" "$variant"
            separator=",
"
        done
        printf '\n  ]\n}'
    } >"$work/rec/raw"
}
oci=application/vnd.oci.image.index.v1+json
echo 0 >"$work/rec/raw-status"

index "$oci" linux/amd64 linux/arm64
rm -f "$work/rec/docker-log"
expect pass "adversary_manifest_list_needs_both_architectures: an OCI index of amd64 and arm64" manifest-list "$listed"
cases=$((cases + 1))
grep -Fqx "buildx imagetools inspect --raw $listed" "$work/rec/docker-log" ||
    fail "manifest-list: docker did not read the pinned list: $(cat "$work/rec/docker-log")"
index application/vnd.docker.distribution.manifest.list.v2+json linux/arm64/v8 linux/amd64
expect pass "adversary_manifest_list_needs_both_architectures: a Docker list of arm64/v8 and amd64" manifest-list "$listed"
printf '%s' "$(tr -d ' \n' <"$work/rec/raw")" >"$work/rec/raw"
expect pass "adversary_manifest_list_needs_both_architectures: the same list without whitespace" manifest-list "$listed"
printf '%s\n' '{"schemaVersion":2,"mediaType":"application/vnd.docker.distribution.manifest.list.v2+json","manifests":[{"mediaType":"application/vnd.docker.distribution.manifest.v2+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000001","size":1,"platform":{"architecture":"arm64","os":"linux","variant":"v8"}},{"mediaType":"application/vnd.docker.distribution.manifest.v2+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000002","size":1,"platform":{"architecture":"amd64","os":"linux"}}]}' >"$work/rec/raw"
expect pass "adversary_manifest_list_needs_both_architectures: architecture named before os" manifest-list "$listed"

for platforms in "linux/amd64" "linux/arm64" "linux/amd64 linux/amd64" "linux/amd64 linux/arm64 linux/arm/v7" \
    "linux/amd64 linux/arm64 unknown/unknown" "linux/amd64 darwin/arm64" ""; do
    # shellcheck disable=SC2086
    index "$oci" $platforms
    expect refuse "adversary_manifest_list_needs_both_architectures: a list of '$platforms'" manifest-list "$listed"
done
index "$oci" linux/amd64 linux/arm64
printf '%s\n' '{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000001","size":1,"platform":{"os":"linux","architecture":"amd64"}},{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000002","size":1,"platform":{"os":"linux","architecture":"arm64"}},{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000003","size":1}]}' >"$work/rec/raw"
expect refuse "adversary_manifest_list_needs_both_architectures: a third entry with no platform" manifest-list "$listed"
printf '%s\n' '{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"application/vnd.oci.image.config.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000001","size":1,"platform":{"os":"linux","architecture":"amd64"}},"layers":[{"mediaType":"application/vnd.oci.image.layer.v1.tar+gzip","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000002","size":1,"platform":{"os":"linux","architecture":"arm64"}}]}' >"$work/rec/raw"
expect refuse "adversary_manifest_list_needs_both_architectures: one image's manifest, not a list" manifest-list "$listed"

index "$oci" linux/amd64 linux/arm64
echo 1 >"$work/rec/raw-status"
expect refuse "adversary_manifest_list_needs_both_architectures: the digest resolves to nothing" manifest-list "$listed"
echo 0 >"$work/rec/raw-status"
for unpinned in ghcr.io/beyond10x/spindle:0.26.0 "ghcr.io/beyond10x/spindle@sha256:abc" \
    "ghcr.io/beyond10x/spindle@sha256:$(printf '%064d' 0 | tr '0' 'A')"; do
    rm -f "$work/rec/docker-log"
    expect refuse "adversary_manifest_list_needs_both_architectures: $unpinned is not pinned" manifest-list "$unpinned"
    cases=$((cases + 1))
    [ ! -e "$work/rec/docker-log" ] || fail "manifest-list: docker read $unpinned, which is not pinned"
done

# --- images-line -----------------------------------------------------------------------------
# adversary_images_line_is_pinned: IMAGES holds one line, the release's image by tag and digest.

repository=ghcr.io/beyond10x/spindle
hex=9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
printf '%s:0.26.0@sha256:%s\n' "$repository" "$hex" >"$work/IMAGES"
expect pass "adversary_images_line_is_pinned: the image by tag and digest" images-line "$work/IMAGES" "$repository" 0.26.0
cases=$((cases + 1))
grep -Fq "$repository:0.26.0@sha256:$hex" "$work/out" || fail "images-line: the pass line does not name the image: $(cat "$work/out")"
for line in "$repository:0.26.0" "$repository@sha256:$hex" "$repository:0.25.0@sha256:$hex" \
    "ghcr.io/beyond10x/other:0.26.0@sha256:$hex" "$repository:0.26.0@sha256:${hex%?}" "$repository:0.26.0@sha256:${hex}0" \
    "$repository:0.26.0@sha256:$(printf '%s' "$hex" | tr 'a-f' 'A-F')" "$repository:0.26.0@sha512:$hex" \
    "$repository:0.26.0@sha256:$hex " " $repository:0.26.0@sha256:$hex" "$repository:0.26.0@sha256:null" ""; do
    printf '%s\n' "$line" >"$work/IMAGES"
    expect refuse "adversary_images_line_is_pinned: IMAGES holds '$line'" images-line "$work/IMAGES" "$repository" 0.26.0
done
printf '%s:0.26.0@sha256:%s\n%s:0.26.0@sha256:%s\n' "$repository" "$hex" "$repository" "$hex" >"$work/IMAGES"
expect refuse "adversary_images_line_is_pinned: IMAGES holds the line twice" images-line "$work/IMAGES" "$repository" 0.26.0
: >"$work/IMAGES"
expect refuse "adversary_images_line_is_pinned: IMAGES is empty" images-line "$work/IMAGES" "$repository" 0.26.0
expect refuse "adversary_images_line_is_pinned: no IMAGES" images-line "$work/missing" "$repository" 0.26.0

# --- the workflow's target lists -------------------------------------------------------------

# The rows of one job's matrix: the `- target:` lines between the job's key and the next job's.
job() {
    sed -n "/^  $1:\$/,/^  [a-z][a-z-]*:\$/p" "$workflow"
}
matrix=$(job build | sed -n 's/^ *- target: *//p' | sort)
lists=$(sed -n 's/^ *targets="\(.*\)"$/\1/p' "$workflow")
cases=$((cases + 1))
[ "$(printf '%s\n' "$lists" | wc -l)" -eq 2 ] || fail "release.yml: expected the sbom and publish target lists, found: $lists"
printf '%s\n' "$lists" | while IFS= read -r list; do
    [ "$(printf '%s\n' $list | sort)" = "$matrix" ] || {
        echo "release-linux-binary-adversary: FAIL: release.yml: a target list ($list) is not the build matrix ($(echo $matrix))" >&2
        exit 1
    }
done || failures=$((failures + 1))
count=$(printf '%s\n' "$matrix" | wc -l | tr -d ' ')
cases=$((cases + 1))
grep -Fq "\"cdx:rustc:sbom:target:triple\")] | length) == $count" "$workflow" ||
    fail "release.yml: the SBOM count of target triples is not $count, the number of build rows"
cases=$((cases + 1))
grep -Fq 'name: b10x-spindle-${{ needs.provenance.outputs.version }}-${{ matrix.target }}.tar.gz' "$workflow" &&
    grep -Fq 'pattern: b10x-spindle-*.tar.gz' "$workflow" ||
    fail "release.yml: the build artifact name and the publish download pattern no longer match"

# --- the workflow's image jobs ---------------------------------------------------------------

# The image job builds one image per gnu row of the build matrix, on that row's platform.
gnu=$(printf '%s\n' "$matrix" | grep -- '-unknown-linux-gnu$' || true)
image_rows=$(job image | sed -n 's/^ *- target: *//p' | sort)
cases=$((cases + 1))
[ -n "$gnu" ] && [ "$image_rows" = "$gnu" ] ||
    fail "release.yml: the image job's rows ($(printf '%s' "$image_rows" | tr '\n' ' ')) are not the build matrix's gnu rows ($(printf '%s' "$gnu" | tr '\n' ' '))"
cases=$((cases + 1))
pairs=$(job image | sed -n 's/^ *- target: *//p; s/^ *platform: *//p' | paste - - | sort)
[ "$pairs" = "$(printf 'aarch64-unknown-linux-gnu\tlinux/arm64\nx86_64-unknown-linux-gnu\tlinux/amd64')" ] ||
    fail "release.yml: the image job's platforms do not match its targets: $pairs"

# adversary_no_push_from_a_pull_request: every step of the image job that logs in or pushes is
# skipped on a pull request, and the job that tags the list runs on no pull request.
guard="if: \${{ github.event_name != 'pull_request' }}"
unguarded=$(job image | awk -v guard="$guard" '
    function flush() { if (step != "" && step ~ /(docker login|push=true|--push|imagetools create)/ && index(step, guard) == 0) print name }
    /^      - / { flush(); step = ""; name = $0 }
    { step = step "\n" $0 }
    END { flush() }
')
cases=$((cases + 1))
[ -z "$unguarded" ] || fail "release.yml: an image step pushes on a pull request: $unguarded"
cases=$((cases + 1))
job image | grep -Eq 'docker login|push=true' || fail "release.yml: the image job pushes nothing"
cases=$((cases + 1))
job image-manifest | grep -Fqx "    $guard" ||
    fail "release.yml: the image-manifest job runs on a pull request"
for name in image image-manifest; do
    cases=$((cases + 1))
    job "$name" | grep -q '^    needs: \[.*\bgate\b.*\]$' || fail "release.yml: the $name job does not wait for the gate"
done
cases=$((cases + 1))
job image-manifest | grep -q '^    needs: \[.*\bimage\b.*\]$' || fail "release.yml: image-manifest does not wait for the image job"
cases=$((cases + 1))
[ "$(grep -c 'packages: write' "$workflow")" -eq 2 ] && job image | grep -Fq 'packages: write' &&
    job image-manifest | grep -Fq 'packages: write' ||
    fail "release.yml: packages: write is not on the image and image-manifest jobs alone"

# IMAGES is listed in SHA256SUMS and uploaded beside it.
cases=$((cases + 1))
job publish | grep -q '^    needs: \[.*\bimage-manifest\b.*\]$' || fail "release.yml: publish does not wait for image-manifest"
cases=$((cases + 1))
job publish | grep -Fq '"IMAGES")' || fail "release.yml: publish does not list IMAGES in SHA256SUMS"
cases=$((cases + 1))
job publish | grep -q 'gh release upload "\$TAG" .*IMAGES SHA256SUMS$' || fail "release.yml: publish does not upload IMAGES"
cases=$((cases + 1))
sed -n '/^  pull_request:$/,/^[a-z]/p' "$workflow" | grep -Fqx '      - Containerfile' ||
    fail "release.yml: a pull request that changes the Containerfile does not run the image jobs"

echo "release-linux-binary-adversary: $cases cases, $failures failed"
[ "$failures" -eq 0 ]
