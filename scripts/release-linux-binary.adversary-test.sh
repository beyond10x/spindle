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

# Given the two checked images' digests, each platform's entry must name its own.
d1=sha256:$(printf '%064d' 1)
d2=sha256:$(printf '%064d' 2)
d3=sha256:$(printf '%064d' 3)
printf '{"schemaVersion":2,"mediaType":"%s","manifests":[{"mediaType":"m","digest":"%s","size":1,"platform":{"os":"linux","architecture":"amd64"}},{"mediaType":"m","digest":"%s","size":1,"annotations":{"a":"b"},"platform":{"architecture":"arm64","os":"linux","variant":"v8"}}],"annotations":{"c":"d"}}' \
    "$oci" "$d1" "$d2" >"$work/rec/raw"
expect pass "adversary_manifest_list_needs_both_architectures: each platform names its checked image" \
    manifest-list "$listed" "$d1" "$d2"
for pair in "$d2 $d1" "$d1 $d3" "$d3 $d2" "$d1 $d1" "$d1 ${d2%?}" "$d1 sha512:${d2#sha256:}"; do
    # shellcheck disable=SC2086
    expect refuse "adversary_manifest_list_needs_both_architectures: the checked images are '$pair'" \
        manifest-list "$listed" $pair
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
[ "$(printf '%s\n' "$lists" | wc -l)" -eq 2 ] || fail "release.yml: expected the sbom and sign target lists, found: $lists"
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

# The image and image-push jobs build one image per gnu row of the build matrix, on that row's
# platform.
gnu=$(printf '%s\n' "$matrix" | grep -- '-unknown-linux-gnu$' || true)
for name in image image-push; do
    rows=$(job "$name" | sed -n 's/^ *- target: *//p' | sort)
    cases=$((cases + 1))
    [ -n "$gnu" ] && [ "$rows" = "$gnu" ] ||
        fail "release.yml: the $name job's rows ($(printf '%s' "$rows" | tr '\n' ' ')) are not the build matrix's gnu rows ($(printf '%s' "$gnu" | tr '\n' ' '))"
    cases=$((cases + 1))
    pairs=$(job "$name" | sed -n 's/^ *- target: *//p; s/^ *platform: *//p' | paste - - | sort)
    [ "$pairs" = "$(printf 'aarch64-unknown-linux-gnu\tlinux/arm64\nx86_64-unknown-linux-gnu\tlinux/amd64')" ] ||
        fail "release.yml: the $name job's platforms do not match its targets: $pairs"
done

# adversary_no_push_from_a_pull_request: the image job, which runs on a pull request, holds no
# package scope and neither logs in nor pushes; image-push and image-manifest, the two jobs that
# hold packages: write, run on no pull request.
cases=$((cases + 1))
! job image | grep -Eq 'packages:|docker login|push=true|--push|imagetools create' ||
    fail "release.yml: the image job, which runs on a pull request, holds package scope, logs in or pushes"
for name in image-push image-manifest; do
    cases=$((cases + 1))
    job "$name" | grep -Eq "^    if: \\$\\{\\{ github.event_name != 'pull_request'( && [^|]*)? \\}\\}$" ||
        fail "release.yml: the $name job runs on a pull request"
done
cases=$((cases + 1))
job image-push | grep -Eq 'docker login' && job image-push | grep -Fq 'push=true' ||
    fail "release.yml: the image-push job pushes nothing"
cases=$((cases + 1))
[ "$(grep -c 'packages: write' "$workflow")" -eq 2 ] && job image-push | grep -Fq 'packages: write' &&
    job image-manifest | grep -Fq 'packages: write' ||
    fail "release.yml: packages: write is not on the image-push and image-manifest jobs alone"
for pair in "image gate" "image-push gate" "image-push image" "image-manifest gate" "image-manifest image-push" \
    "publish image-manifest" "sign gate" "sign build" "sign sbom" "sign image-manifest" "publish sign"; do
    name=${pair% *}
    need=${pair#* }
    cases=$((cases + 1))
    job "$name" | grep -Eq "^    needs: \\[(.*, )?$need(, .*)?\\]$" || fail "release.yml: the $name job does not wait for $need"
done
# image-push pushes the image the image job checked: it reads the ID the image job recorded and
# refuses a rebuild with another before it logs in.
cases=$((cases + 1))
job image | grep -Fq 'name: image-checked-${{ matrix.target }}' && job image-push | grep -Fq 'name: image-checked-${{ matrix.target }}' &&
    job image-push | grep -Fq 'if [ "$rebuilt" != "$checked" ]; then' ||
    fail "release.yml: image-push does not compare its rebuild with the image the image job checked"
cases=$((cases + 1))
sed -n '/^  pull_request:$/,/^[a-z]/p' "$workflow" | grep -Fqx '      - Containerfile' ||
    fail "release.yml: a pull request that changes the Containerfile does not run the image jobs"

# A step's run block, by the job and the start of the step's name.
step_script() {
    job "$1" | awk -v want="name: $2" '
        (index($0, "      - " want) == 1 || index($0, "        " want) == 1) { found = 1; next }
        found && /^        run: \|$/ { body = 1; next }
        body && /^          / { print substr($0, 11); next }
        body && /^$/ { print ""; next }
        body { exit }
    ' >"$3"
    [ -s "$3" ] || fail "release.yml: the $1 job has no step '$2' with a run block"
}

# --- a tag whose commit has no Containerfile -------------------------------------------------
# adversary_backfill_without_containerfile: a backfilled tag cut before the image existed builds
# no image and publishes the rest without IMAGES; a tag with one carries IMAGES.

for name in image image-push image-manifest; do
    cases=$((cases + 1))
    job "$name" | grep -Eq "^    if: .*needs\\.provenance\\.outputs\\.containerfile == 'true'" ||
        fail "release.yml: the $name job runs for a commit without a Containerfile"
done
publish_if=$(job publish | sed -n 's/^    if: //p')
cases=$((cases + 1))
case $publish_if in
    *"needs.image-manifest.result == 'success' || (needs.provenance.outputs.containerfile == 'false' && needs.image-manifest.result == 'skipped')"*) ;;
    *) fail "release.yml: publish does not run once image-manifest is skipped for a commit without a Containerfile: $publish_if" ;;
esac
cases=$((cases + 1))
[ "$(printf '%s' "$publish_if" | grep -o "result == 'skipped'" | grep -c '' || true)" -eq 1 ] ||
    fail "release.yml: publish runs on another skipped job: $publish_if"
cases=$((cases + 1))
job publish | grep -Eq "^    if: .*needs\\.gate\\.result == 'success' && needs\\.build\\.result == 'success' && needs\\.sbom\\.result == 'success'" ||
    fail "release.yml: publish does not need a green gate, build and sbom: $publish_if"
# sign, which publish waits for, runs on the same answers: a commit without a Containerfile is
# signed without IMAGES, and nothing is signed past a failed gate, build or sbom.
sign_if=$(job sign | sed -n 's/^    if: //p')
cases=$((cases + 1))
case $sign_if in
    *"needs.gate.result == 'success' && needs.build.result == 'success' && needs.sbom.result == 'success' && (needs.image-manifest.result == 'success' || (needs.provenance.outputs.containerfile == 'false' && needs.image-manifest.result == 'skipped'))"*) ;;
    *) fail "release.yml: sign does not run on a green gate, build and sbom, with IMAGES or for a commit without a Containerfile: $sign_if" ;;
esac
cases=$((cases + 1))
case $publish_if in
    *"needs.sign.result == 'success'"*) ;;
    *) fail "release.yml: publish runs without a signed release: $publish_if" ;;
esac

mkdir -p "$work/steps"
step_script provenance "Whether the commit builds an image" "$work/steps/containerfile.sh"
# provenance's answer for a commit of <version>, with or without a root Containerfile: true,
# false, or the step fails naming the file (the answer is then "fails").
provenance_answer() {
    rm -rf "$work/commit"
    mkdir -p "$work/commit"
    [ "$2" = without ] || cp "$containerfile" "$work/commit/Containerfile"
    : >"$work/commit/output"
    if (cd "$work/commit" && GITHUB_OUTPUT=$work/commit/output GITHUB_STEP_SUMMARY=$work/commit/summary VERSION=$1 \
        bash "$work/steps/containerfile.sh" >/dev/null 2>"$work/commit/err"); then
        sed -n 's/^containerfile=//p' "$work/commit/output"
    elif [ ! -s "$work/commit/output" ] && grep -q 'no Containerfile at the repository root' "$work/commit/err"; then
        echo fails
    else
        echo "broken: $(cat "$work/commit/output" "$work/commit/err")"
    fi
}
for row in "0.26.0 with true" "0.25.3 with true" "0.25.3 without false" "0.9.0 without false" \
    "0.26.0 without fails" "0.26.1 without fails" "0.100.0 without fails" "1.0.0 without fails"; do
    set -- $row
    cases=$((cases + 1))
    answer=$(provenance_answer "$1" "$2")
    [ "$answer" = "$3" ] ||
        fail "release.yml: provenance answers '$answer' for $1 $2 a Containerfile, not '$3'"
done
for version in "" "0.26" "v0.26.0"; do
    cases=$((cases + 1))
    answer=$(provenance_answer "$version" without)
    case $answer in
        broken:*) [ ! -s "$work/commit/output" ] || fail "release.yml: provenance answers for the version '$version': $answer" ;;
        *) fail "release.yml: provenance answers '$answer' for the version '$version'" ;;
    esac
done

# sign's checksum step (publish's before story:release-artifacts-signed) on the downloaded
# artifacts, with and without IMAGES.
step_script sign "Check each artifact against the hash its job wrote" "$work/steps/sums.sh"
sums() {
    expected=$1
    with=$2
    rm -rf "$work/dist"
    mkdir -p "$work/dist"
    for file in b10x-spindle-0.26.0-x86_64-unknown-linux-gnu.tar.gz b10x-spindle-0.26.0-aarch64-unknown-linux-gnu.tar.gz \
        b10x-spindle-0.26.0-x86_64-unknown-linux-musl.tar.gz b10x-spindle-0.26.0-aarch64-apple-darwin.tar.gz \
        b10x-spindle-0.26.0-x86_64-apple-darwin.tar.gz b10x-spindle-0.26.0.cdx.json $with; do
        case $file in
            IMAGES) printf 'ghcr.io/beyond10x/spindle:0.26.0@sha256:%s\n' "$hex" >"$work/dist/$file" ;;
            *) printf '%s\n' "$file" >"$work/dist/$file" ;;
        esac
        (cd "$work/dist" && sha256sum "$file" >"$file.sha256")
    done
    (cd "$work/dist" && VERSION=0.26.0 IMAGES_EXPECTED=$expected bash "$work/steps/sums.sh" >"$work/sums.out" 2>&1)
}
cases=$((cases + 1))
sums false "" && [ "$(grep -c '' "$work/dist/SHA256SUMS")" -eq 6 ] && ! grep -q IMAGES "$work/dist/SHA256SUMS" ||
    fail "release.yml: publish does not sum the six assets of a commit without a Containerfile: $(cat "$work/sums.out")"
cases=$((cases + 1))
sums true IMAGES && [ "$(grep -c '' "$work/dist/SHA256SUMS")" -eq 7 ] && grep -q ' IMAGES$' "$work/dist/SHA256SUMS" ||
    fail "release.yml: publish does not list IMAGES in SHA256SUMS: $(cat "$work/sums.out")"
cases=$((cases + 1))
! sums true "" || fail "release.yml: publish passes a release with an image but no IMAGES"
cases=$((cases + 1))
! sums false IMAGES || fail "release.yml: publish passes an IMAGES nobody expected"
cases=$((cases + 1))
! sums "" IMAGES || fail "release.yml: publish passes when it does not know whether the commit has an image"

# cosign: sign-blob writes the sha256 of the blob as its bundle (and fails on the asset named in
# cosign-fail-on), verify-blob passes only a blob whose sha256 is the bundle's; sign and verify of
# an image print it. Every call is appended to cosign-log.
mkdir -p "$work/sigtools"
printf '#!%s\nrec=%s\n' "$shell" "$work/rec" >"$work/sigtools/cosign"
cat >>"$work/sigtools/cosign" <<'STUB'
printf '%s\n' "$*" >>"$rec/cosign-log"
command=$1
shift
bundle=""
subject=""
while [ $# -gt 0 ]; do
    case $1 in
        --bundle | --certificate-identity | --certificate-oidc-issuer) [ $# -ge 2 ] || exit 2; [ "$1" != --bundle ] || bundle=$2; shift 2 ;;
        --*) shift ;;
        *) subject=$1; shift ;;
    esac
done
case $command in
    sign-blob)
        [ -n "$bundle" ] && [ -f "$subject" ] || exit 2
        if [ -f "$rec/cosign-fail-on" ] && [ "$(basename "$subject")" = "$(cat "$rec/cosign-fail-on")" ]; then
            echo "stub cosign: no signature for $subject" >&2
            exit 1
        fi
        sha256sum <"$subject" | cut -d ' ' -f 1 >"$bundle"
        ;;
    verify-blob)
        [ -f "$bundle" ] && [ -f "$subject" ] || exit 1
        [ "$(sha256sum <"$subject" | cut -d ' ' -f 1)" = "$(cat "$bundle")" ] || { echo "stub cosign: invalid signature" >&2; exit 1; }
        echo "Verified OK"
        ;;
    sign | verify) [ -n "$subject" ] || exit 2; echo "$command $subject" ;;
    version) echo "stub cosign" ;;
    *) exit 2 ;;
esac
STUB
chmod 755 "$work/sigtools/cosign"

# sign's signing step over the checked artifacts of sums, with IMAGES where expected.
step_script sign "Sign each asset" "$work/steps/sign.sh"
signed() {
    sums "$1" "$([ "$1" = false ] || echo IMAGES)" >/dev/null 2>&1 || return 1
    rm -rf "$work/signrun"
    mkdir -p "$work/signrun"
    rm -f "$work/rec/cosign-log"
    (cd "$work/dist" && PATH="$work/sigtools:$PATH" RUNNER_TEMP=$work/signrun \
        GITHUB_STEP_SUMMARY=$work/signrun/summary bash "$work/steps/sign.sh" >"$work/sign.out" 2>&1)
}
# The assets of a release of 0.26.0, with IMAGES where $1 is true, one per line.
assets_of() {
    for target in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-unknown-linux-musl \
        aarch64-apple-darwin x86_64-apple-darwin; do
        echo "b10x-spindle-0.26.0-$target.tar.gz"
    done
    echo b10x-spindle-0.26.0.cdx.json
    [ "$1" = false ] || echo IMAGES
    echo SHA256SUMS
}

# adversary_publish_uploads_a_bundle_per_asset: publish's upload step with a gh stub uploads each
# asset and its bundle, IMAGES and its bundle only where expected, and refuses an asset or a
# bundle already on the page. gh: `release view` answers the names in page-names, `release
# upload` is appended to gh-upload, `release download` copies page/ into --dir, and `attestation
# verify` passes a subject whose sha256 is in attested, naming the commit in attested-commit.
step_script publish "Upload the archives" "$work/steps/upload.sh"
mkdir -p "$work/ghtools"
printf '#!%s\nrec=%s\n' "$shell" "$work/rec" >"$work/ghtools/gh"
cat >>"$work/ghtools/gh" <<'STUB'
printf '%s\n' "$*" >>"$rec/gh-log"
case "$1 $2" in
    "release upload") printf '%s\n' "$*" >>"$rec/gh-upload" ;;
    "release view") cat "$rec/page-names" 2>/dev/null || true ;;
    "release download")
        dir=""
        while [ $# -gt 0 ]; do
            case $1 in --dir) dir=$2; shift 2 ;; *) shift ;; esac
        done
        [ -n "$dir" ] || exit 2
        mkdir -p "$dir"
        cp "$rec/page/"* "$dir/"
        ;;
    "attestation verify")
        case $3 in
            oci://*) sum=${3##*@sha256:} ;;
            *) [ -f "$3" ] || exit 1; sum=$(sha256sum <"$3" | cut -d ' ' -f 1) ;;
        esac
        grep -Fqx "$sum" "$rec/attested" || { echo "stub gh: no attestation for $3" >&2; exit 1; }
        printf '[{"verificationResult":{"signature":{"certificate":{"buildSignerURI":"https://github.com/beyond10x/spindle/.github/workflows/release.yml@stub","sourceRepositoryDigest":"%s"}}}}]\n' \
            "$(cat "$rec/attested-commit")"
        ;;
esac
exit 0
STUB
chmod 755 "$work/ghtools/gh"
upload() {
    rm -f "$work/rec/gh-upload"
    (cd "$work/dist" && PATH="$work/ghtools:$PATH" TAG=0.26.0 IMAGES_EXPECTED=$1 \
        GITHUB_STEP_SUMMARY=$work/upload.summary bash "$work/steps/upload.sh" >"$work/upload.out" 2>&1)
}
for expected in true false; do
    rm -f "$work/rec/page-names"
    cases=$((cases + 1))
    signed "$expected" || fail "adversary_publish_uploads_a_bundle_per_asset: sign fails with IMAGES_EXPECTED=$expected: $(cat "$work/sign.out" "$work/sums.out")"
    cases=$((cases + 1))
    upload "$expected" ||
        fail "adversary_publish_uploads_a_bundle_per_asset: publish's upload step fails with IMAGES_EXPECTED=$expected: $(cat "$work/upload.out")"
    uploaded=$(cat "$work/rec/gh-upload" 2>/dev/null || true)
    want=$(assets_of "$expected" | sed 'p; s/$/.sigstore.json/' | sort)
    cases=$((cases + 1))
    case $uploaded in
        "release upload 0.26.0 "*) [ "$(printf '%s\n' ${uploaded#release upload 0.26.0 } | sort)" = "$want" ] ;;
        *) false ;;
    esac || fail "adversary_publish_uploads_a_bundle_per_asset: publish uploads '$uploaded' with IMAGES_EXPECTED=$expected, not the $(printf '%s\n' "$want" | grep -c '') files: $(echo $want)"
    for present in SHA256SUMS b10x-spindle-0.26.0.cdx.json.sigstore.json; do
        printf 'b10x-spindle-0.26.0-x86_64-apple-darwin.tar.gz.extra\n%s\n' "$present" >"$work/rec/page-names"
        cases=$((cases + 1))
        ! upload "$expected" && [ ! -e "$work/rec/gh-upload" ] ||
            fail "adversary_publish_uploads_a_bundle_per_asset: publish uploads over $present already on the page"
    done
    rm -f "$work/rec/page-names"
done
# An asset whose bundle is missing is not uploaded, and nor is anything else.
signed true >/dev/null 2>&1 || true
rm -f "$work/dist/IMAGES.sigstore.json"
cases=$((cases + 1))
! upload true && [ ! -e "$work/rec/gh-upload" ] ||
    fail "adversary_publish_uploads_a_bundle_per_asset: publish uploads a release whose IMAGES has no bundle"
signed true >/dev/null 2>&1 || true
printf 'x\n' >"$work/dist/b10x-spindle-0.26.0.cdx.json"
cases=$((cases + 1))
! upload true && [ ! -e "$work/rec/gh-upload" ] ||
    fail "adversary_publish_uploads_a_bundle_per_asset: publish uploads an SBOM that is not the one SHA256SUMS lists"

# adversary_sign_signs_each_asset: sign's signing step writes one <asset>.sigstore.json per asset
# and calls cosign sign-blob once per asset; a cosign that fails on one asset fails the step.
for expected in true false; do
    cases=$((cases + 1))
    if ! signed "$expected"; then
        fail "adversary_sign_signs_each_asset: the step fails with IMAGES_EXPECTED=$expected: $(cat "$work/sign.out")"
        continue
    fi
    want=$(assets_of "$expected" | sort)
    cases=$((cases + 1))
    [ "$(cd "$work/dist" && ls -- *.sigstore.json | sed 's/\.sigstore\.json$//' | sort)" = "$want" ] ||
        fail "adversary_sign_signs_each_asset: the bundles with IMAGES_EXPECTED=$expected are not one per asset: $(cd "$work/dist" && ls)"
    cases=$((cases + 1))
    [ "$(sed -n 's/^sign-blob .* //p' "$work/rec/cosign-log" | sort)" = "$want" ] &&
        [ "$(grep -c '^sign-blob ' "$work/rec/cosign-log")" -eq "$(printf '%s\n' "$want" | grep -c '')" ] ||
        fail "adversary_sign_signs_each_asset: cosign sign-blob is not called once per asset with IMAGES_EXPECTED=$expected: $(cat "$work/rec/cosign-log")"
    cases=$((cases + 1))
    grep -Fqx 'sign-blob --yes --bundle SHA256SUMS.sigstore.json SHA256SUMS' "$work/rec/cosign-log" ||
        fail "adversary_sign_signs_each_asset: SHA256SUMS is not signed into SHA256SUMS.sigstore.json: $(cat "$work/rec/cosign-log")"
done
for broken in b10x-spindle-0.26.0.cdx.json IMAGES SHA256SUMS; do
    printf '%s\n' "$broken" >"$work/rec/cosign-fail-on"
    cases=$((cases + 1))
    ! signed true || fail "adversary_sign_signs_each_asset: the step passes though cosign fails on $broken"
done
rm -f "$work/rec/cosign-fail-on"

# --- a tag already in ghcr.io ----------------------------------------------------------------
# adversary_existing_tag_is_done_or_refused: image-manifest leaves a tag that lists exactly the
# two checked images as it is and writes IMAGES for it, refuses one that lists other images, and
# tags a version ghcr.io does not hold. The docker stub answers --raw with the list it holds and
# resolves a pinned reference only to the list whose bytes hash to its digest.

step_script image-manifest "Tag the manifest list" "$work/steps/tag.sh"
mkdir -p "$work/mytools"
printf '#!%s\ndir=%s\n' "$shell" "$work/mytag" >"$work/mytools/docker"
cat >>"$work/mytools/docker" <<'STUB'
printf '%s\n' "$*" >>"$dir/docker-log"
case "$*" in
    "buildx imagetools create"*) cp "$dir/created" "$dir/held"; exit 0 ;;
    "buildx imagetools inspect --raw "*)
        [ -f "$dir/held" ] || exit 1
        case $5 in
            *@sha256:*) [ "${5##*@sha256:}" = "$(sha256sum <"$dir/held" | cut -d ' ' -f 1)" ] || exit 1 ;;
        esac
        cat "$dir/held"
        ;;
    "buildx imagetools inspect "*)
        [ -f "$dir/held" ] || { echo "ERROR: $4: not found" >&2; exit 1; }
        echo "Name: $4"
        ;;
    *) exit 1 ;;
esac
STUB
chmod 755 "$work/mytools/docker"
list() {
    printf '{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:%064d","size":1,"platform":{"architecture":"amd64","os":"linux"}},{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:%064d","size":1,"platform":{"architecture":"arm64","os":"linux"}}]}' "$1" "$2"
}
mytag() {
    rm -rf "$work/mytag"
    mkdir -p "$work/mytag/digests" "$work/mytag/scripts"
    ln -s "$subject" "$work/mytag/scripts/release-linux-binary.sh"
    printf 'sha256:%064d\n' 1 >"$work/mytag/digests/x86_64-unknown-linux-gnu"
    printf 'sha256:%064d\n' 2 >"$work/mytag/digests/aarch64-unknown-linux-gnu"
    list 1 2 >"$work/mytag/created"
    [ -z "$1" ] || list $1 >"$work/mytag/held"
    : >"$work/mytag/docker-log"
    (cd "$work/mytag" && PATH="$work/mytools:$PATH" IMAGE=ghcr.io/beyond10x/spindle VERSION=0.26.0 \
        GITHUB_STEP_SUMMARY="$work/mytag/summary" GITHUB_REPOSITORY=beyond10x/spindle \
        bash "$work/steps/tag.sh" >"$work/mytag/out" 2>&1)
}
held_digest() {
    list 1 2 | sha256sum | cut -d ' ' -f 1
}

cases=$((cases + 1))
mytag "" || fail "adversary_existing_tag_is_done_or_refused: a new version is not tagged: $(cat "$work/mytag/out")"
cases=$((cases + 1))
[ "$(grep -c 'imagetools create --tag ghcr.io/beyond10x/spindle:0.26.0 ' "$work/mytag/docker-log")" -eq 1 ] ||
    fail "adversary_existing_tag_is_done_or_refused: a new version is not tagged once: $(cat "$work/mytag/docker-log")"
cases=$((cases + 1))
[ "$(cat "$work/mytag/images/IMAGES" 2>/dev/null)" = "ghcr.io/beyond10x/spindle:0.26.0@sha256:$(held_digest)" ] &&
    [ -s "$work/mytag/images/IMAGES.sha256" ] ||
    fail "adversary_existing_tag_is_done_or_refused: IMAGES of a new tag is not the list's digest: $(cat "$work/mytag/images/IMAGES" 2>/dev/null)"

cases=$((cases + 1))
mytag "1 2" || fail "adversary_existing_tag_is_done_or_refused: a tag that lists the two checked images fails: $(cat "$work/mytag/out")"
cases=$((cases + 1))
! grep -q 'imagetools create' "$work/mytag/docker-log" ||
    fail "adversary_existing_tag_is_done_or_refused: a tag that lists the two checked images is tagged again"
cases=$((cases + 1))
[ "$(cat "$work/mytag/images/IMAGES" 2>/dev/null)" = "ghcr.io/beyond10x/spindle:0.26.0@sha256:$(held_digest)" ] ||
    fail "adversary_existing_tag_is_done_or_refused: IMAGES of a tag already done is not its list's digest: $(cat "$work/mytag/images/IMAGES" 2>/dev/null)"

for held in "3 4" "1 3" "2 1"; do
    cases=$((cases + 1))
    ! mytag "$held" || fail "adversary_existing_tag_is_done_or_refused: a tag that lists the images '$held' passes"
    cases=$((cases + 1))
    ! grep -q 'imagetools create' "$work/mytag/docker-log" && [ ! -e "$work/mytag/images/IMAGES" ] ||
        fail "adversary_existing_tag_is_done_or_refused: a tag that lists the images '$held' is moved or written to IMAGES"
done


# --- adversary_w31_U2 ------------------------------------------------------------------------
# Cases of wave 31's adversary on story:spindle-image-published.

# A setuid or setgid binary owned by root (COPY's default owner) runs as UID 0 under any USER: the
# kernel honours the bit, since docker run sets no no-new-privileges by default.
for mode in 4555 2555 6555 u+s; do
    recipe "FROM $base" "COPY --chmod=$mode b10x-spindle /b10x-spindle" 'USER 65532:65532' 'ENTRYPOINT ["/b10x-spindle"]'
    expect refuse "adversary_w31_U2: COPY --chmod=$mode makes the entrypoint setuid or setgid root" \
        containerfile "$work/Containerfile"
done

# Docker reads an escape followed by spaces or tabs as a line continuation
# (lineContinuationRegex `\\[ \t]*$` in BuildKit's Dockerfile parser), so the check's "no
# continuation line" holds only where the backslash is the line's last byte.
for trailing in ' ' '	'; do
    good_recipe
    printf 'LABEL a=b \\%s\n' "$trailing" >>"$work/Containerfile"
    expect refuse "adversary_w31_U2: a continuation line whose backslash is followed by whitespace" \
        containerfile "$work/Containerfile"
done

# FROM with a flag before a pinned image: refused only by the check for a space in FROM's
# argument, which no other case reaches (a stage name also breaks the digest's length).
recipe "FROM --platform=linux/amd64 $base" 'COPY b10x-spindle /b10x-spindle' 'USER 65532' 'ENTRYPOINT ["/b10x-spindle"]'
expect refuse "adversary_w31_U2: FROM with --platform before a pinned image" containerfile "$work/Containerfile"

# A user with two colons: refused only by numeric_user's `*:*:*`, which no other case reaches.
for user in 65532:65532:65532 65532:65532:0; do
    recipe "FROM $base" 'COPY b10x-spindle /b10x-spindle' "USER $user" 'ENTRYPOINT ["/b10x-spindle"]'
    expect refuse "adversary_w31_U2: USER '$user'" containerfile "$work/Containerfile"
    printf '%s\n' "$user" >"$work/rec/user-out"
    expect refuse "adversary_w31_U2: the image runs as '$user'" image-user "$checked"
done

# A third entry whose digest is not sha256 is not counted, so a list of three entries passes as two.
printf '%s\n' '{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000001","size":1,"platform":{"os":"linux","architecture":"amd64"}},{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000002","size":1,"platform":{"os":"linux","architecture":"arm64"}},{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003","size":1}]}' >"$work/rec/raw"
echo 0 >"$work/rec/raw-status"
expect refuse "adversary_w31_U2: a third entry with a sha512 digest and no platform" manifest-list "$listed"

# images-line with an empty version: the line `<repository>:@sha256:<digest>` names no tag.
printf '%s:@sha256:%s\n' "$repository" "$hex" >"$work/IMAGES"
expect refuse "adversary_w31_U2: IMAGES with an empty version" images-line "$work/IMAGES" "$repository" ""

# The image job's `if` starts with !cancelled(), so no implicit success() guards it: the gate's
# success is its only guard against pushing a commit the gate did not pass, and a skipped gate is
# accepted only on a pull request.
image_if=$(job image | sed -n 's/^    if: //p')
cases=$((cases + 1))
case $image_if in
    *"needs.gate.result == 'success'"*) ;;
    *) fail "adversary_w31_U2: the image job runs without a green gate: $image_if" ;;
esac
cases=$((cases + 1))
skipped=$(printf '%s' "$image_if" | grep -o "needs.gate.result == 'skipped'" | grep -c '' || true)
paired=$(printf '%s' "$image_if" | grep -o "github.event_name == 'pull_request' && needs.gate.result == 'skipped'" | grep -c '' || true)
[ "$skipped" -eq "$paired" ] ||
    fail "adversary_w31_U2: the image job accepts a skipped gate outside a pull request: $image_if"

# The step of image-manifest that refuses a tag already in ghcr.io, run under bash with a docker
# stub: the tag exists, the tag is not found, and the registry cannot be asked.
tag_step=$work/tag-step.sh
job image-manifest | awk '
    /^      - name: Tag the manifest list/ { found = 1; next }
    found && /^        run: \|$/ { body = 1; next }
    body && /^          / { print substr($0, 11); next }
    body { exit }
' >"$tag_step"
mkdir -p "$work/tagtools"
printf '#!%s\ndir=%s\n' "$shell" "$work/tagstep" >"$work/tagtools/docker"
cat >>"$work/tagtools/docker" <<'STUB'
printf '%s\n' "$*" >>"$dir/docker-log"
case "$*" in
    "buildx imagetools create"*) exit 1 ;;
    "buildx imagetools inspect "*" "*) exit 1 ;;
    "buildx imagetools inspect "*)
        case $(cat "$dir/inspect-mode") in
            exists) echo "Name: $4"; exit 0 ;;
            missing) echo "ERROR: $4: not found" >&2; exit 1 ;;
            unreachable) echo "ERROR: failed to do request: Head \"https://ghcr.io/v2/manifests\": dial tcp: i/o timeout" >&2; exit 1 ;;
        esac
        ;;
esac
exit 1
STUB
chmod 755 "$work/tagtools/docker"
tag_run() {
    rm -rf "$work/tagstep"
    mkdir -p "$work/tagstep/digests"
    printf 'sha256:%064d\n' 1 >"$work/tagstep/digests/x86_64-unknown-linux-gnu"
    printf 'sha256:%064d\n' 2 >"$work/tagstep/digests/aarch64-unknown-linux-gnu"
    printf '%s\n' "$1" >"$work/tagstep/inspect-mode"
    : >"$work/tagstep/docker-log"
    (cd "$work/tagstep" && PATH="$work/tagtools:$PATH" IMAGE=ghcr.io/beyond10x/spindle VERSION=0.26.0 \
        GITHUB_STEP_SUMMARY="$work/tagstep/summary" GITHUB_REPOSITORY=beyond10x/spindle \
        bash "$tag_step" >"$work/tagstep/out" 2>"$work/tagstep/err") || true
}
cases=$((cases + 1))
if [ ! -s "$tag_step" ]; then
    fail "adversary_w31_U2: image-manifest has no step 'Tag the manifest list' with a run block"
else
    tag_run exists
    grep -Fqx "buildx imagetools inspect ghcr.io/beyond10x/spindle:0.26.0" "$work/tagstep/docker-log" &&
        ! grep -q 'imagetools create' "$work/tagstep/docker-log" ||
        fail "adversary_w31_U2: image-manifest tags a version already in ghcr.io: $(tr '\n' ';' <"$work/tagstep/docker-log")"
    cases=$((cases + 1))
    tag_run missing
    grep -q 'imagetools create --tag ghcr.io/beyond10x/spindle:0.26.0 ' "$work/tagstep/docker-log" ||
        fail "adversary_w31_U2: image-manifest does not tag a version ghcr.io does not hold: $(tr '\n' ';' <"$work/tagstep/docker-log") $(cat "$work/tagstep/err")"
    cases=$((cases + 1))
    tag_run unreachable
    ! grep -q 'imagetools create' "$work/tagstep/docker-log" ||
        fail "adversary_w31_U2: image-manifest tags the version when ghcr.io cannot say whether it holds it (inspect failed with a timeout, not 'not found'): $(tr '\n' ';' <"$work/tagstep/docker-log")"
fi

# --- adversary_w31_U2, round 2 ---------------------------------------------------------------

# BuildKit drops a line's trailing "\r\n" before it looks for the escape (trimNewline), so a
# backslash before a carriage return is a continuation line too.
good_recipe
printf 'LABEL a=b \\\r\n' >>"$work/Containerfile"
expect refuse "adversary_w31_U2: a continuation line whose backslash is followed by a carriage return" \
    containerfile "$work/Containerfile"

# A tag pushed for a new release whose commit has no Containerfile at the root (moved or renamed)
# is no tag cut before the image existed: provenance must not answer containerfile=false for it,
# or the release publishes green without IMAGES and without an image.
r2=$work/r2
rm -rf "$r2"
mkdir -p "$r2/commit/deploy"
cp "$containerfile" "$r2/commit/deploy/Containerfile"
: >"$r2/commit/output"
cases=$((cases + 1))
if [ ! -s "$work/steps/containerfile.sh" ]; then
    fail "adversary_w31_U2: provenance has no step 'Whether the commit builds an image'"
else
    (cd "$r2/commit" && GITHUB_OUTPUT=$r2/commit/output GITHUB_STEP_SUMMARY=$r2/commit/summary \
        GITHUB_EVENT_NAME=push GITHUB_REF_TYPE=tag GITHUB_REF_NAME=0.26.0 EVENT=push TAG=0.26.0 VERSION=0.26.0 \
        bash "$work/steps/containerfile.sh" >/dev/null 2>&1) || true
    ! grep -Fqx 'containerfile=false' "$r2/commit/output" ||
        fail "adversary_w31_U2: a tag push of 0.26.0 whose Containerfile moved to deploy/ answers containerfile=false, so the release publishes without IMAGES"
fi

# image-push and image-manifest carry no status function in their `if`, so success() is implied
# and a failed or skipped gate, image or image-push keeps them from running.
for name in image-push image-manifest; do
    cases=$((cases + 1))
    if job "$name" | grep '^    if: ' | grep -Eq 'always\(\)|cancelled\(\)|failure\(\)'; then
        fail "adversary_w31_U2: the $name job's if holds a status function, so it runs past a failed need: $(job "$name" | grep '^    if: ')"
    fi
done

# image-manifest's tag step: an inspect error that only contains the words "not found" (here a
# credential helper missing from PATH) is no answer that the tag is absent.
mkdir -p "$r2/tools"
printf '#!%s\ndir=%s\n' "$shell" "$r2/tag" >"$r2/tools/docker"
cat >>"$r2/tools/docker" <<'STUB'
printf '%s\n' "$*" >>"$dir/docker-log"
case "$*" in
    "buildx imagetools create"*) exit 1 ;;
    "buildx imagetools inspect --raw "*) exit 1 ;;
    "buildx imagetools inspect "*)
        echo 'ERROR: error getting credentials - err: exec: "docker-credential-ghcr": executable file not found in $PATH, out: ``' >&2
        exit 1
        ;;
esac
exit 1
STUB
chmod 755 "$r2/tools/docker"
r2_tag() {
    tools=$1
    rm -rf "$r2/tag"
    mkdir -p "$r2/tag/digests" "$r2/tag/scripts"
    ln -s "$subject" "$r2/tag/scripts/release-linux-binary.sh"
    printf 'sha256:%064d\n' 1 >"$r2/tag/digests/x86_64-unknown-linux-gnu"
    printf 'sha256:%064d\n' 2 >"$r2/tag/digests/aarch64-unknown-linux-gnu"
    : >"$r2/tag/docker-log"
    (cd "$r2/tag" && PATH="$tools:$PATH" IMAGE=ghcr.io/beyond10x/spindle VERSION=0.26.0 \
        GITHUB_STEP_SUMMARY="$r2/tag/summary" GITHUB_REPOSITORY=beyond10x/spindle \
        bash "$work/steps/tag.sh" >"$r2/tag/out" 2>&1)
}
cases=$((cases + 1))
r2_tag "$r2/tools" || true
! grep -q 'imagetools create' "$r2/tag/docker-log" ||
    fail "adversary_w31_U2: image-manifest tags the version on an inspect error that only contains 'not found': $(tr '\n' ';' <"$r2/tag/docker-log")"

# An existing tag that lists the two checked images on their platforms and a third entry, with or
# without a platform, is not the list of the two checked images.
for third in '{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000003","size":1}' \
    '{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000003","size":1,"platform":{"architecture":"unknown","os":"unknown"}}' \
    '{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000001","size":1,"platform":{"architecture":"amd64","os":"linux"}}'; do
    # mytag's setup, with ghcr.io holding the two checked images and the third entry.
    rm -rf "$work/mytag"
    mkdir -p "$work/mytag/digests" "$work/mytag/scripts"
    ln -s "$subject" "$work/mytag/scripts/release-linux-binary.sh"
    printf 'sha256:%064d\n' 1 >"$work/mytag/digests/x86_64-unknown-linux-gnu"
    printf 'sha256:%064d\n' 2 >"$work/mytag/digests/aarch64-unknown-linux-gnu"
    list 1 2 >"$work/mytag/created"
    printf '%s]}' "$(list 1 2 | sed 's|]}$||'),$third" >"$work/mytag/held"
    : >"$work/mytag/docker-log"
    cases=$((cases + 1))
    [ "$(grep -o '"digest":' "$work/mytag/held" | grep -c '')" -eq 3 ] ||
        fail "adversary_w31_U2: the probe list holds no third entry: $(cat "$work/mytag/held")"
    cases=$((cases + 1))
    if (cd "$work/mytag" && PATH="$work/mytools:$PATH" IMAGE=ghcr.io/beyond10x/spindle VERSION=0.26.0 \
        GITHUB_STEP_SUMMARY="$work/mytag/summary" GITHUB_REPOSITORY=beyond10x/spindle \
        bash "$work/steps/tag.sh" >"$work/mytag/out" 2>&1); then
        fail "adversary_w31_U2: an existing tag that lists a third entry passes: $third"
    fi
    cases=$((cases + 1))
    ! grep -q 'imagetools create' "$work/mytag/docker-log" && [ ! -e "$work/mytag/images/IMAGES" ] ||
        fail "adversary_w31_U2: an existing tag with a third entry is moved or written to IMAGES"
done

# --- signed release assets -------------------------------------------------------------------
# story:release-artifacts-signed: each asset carries a cosign bundle and a provenance attestation,
# the image is signed by its digest, and only the signing jobs hold id-token: write.

# adversary_id_token_only_on_signing_jobs: in a copy of release.yml, id-token: write is on sign
# and image-manifest only, attestations: write on sign only, nothing is write-all, the top-level
# permissions are contents: read, the workflow runs on push, workflow_dispatch and pull_request
# only, and each job holding id-token: write has one job-level if of an accepted guard form, each
# false on a pull_request event:
#   ${{ github.event_name != 'pull_request' }}
#   ${{ github.event_name != 'pull_request' && <rest> }}
#   ${{ !cancelled() && github.event_name != 'pull_request' && <rest> }}
# where <rest> holds no || outside parentheses and nothing follows the closing }}.
cat >"$work/permissions.awk" <<'AWK'
function trim(s) { sub(/^[ \t]+/, "", s); sub(/[ \t]+$/, "", s); return s }
function guarded(e,    q, a, b, c, rest, depth, i, ch) {
    q = "'"
    a = "${{ github.event_name != " q "pull_request" q " }}"
    b = "${{ github.event_name != " q "pull_request" q " && "
    c = "${{ !cancelled() && github.event_name != " q "pull_request" q " && "
    if (e == a) return 1
    if (index(e, b) == 1) rest = substr(e, length(b) + 1)
    else if (index(e, c) == 1) rest = substr(e, length(c) + 1)
    else return 0
    if (length(rest) < 4 || substr(rest, length(rest) - 2) != " }}") return 0
    rest = substr(rest, 1, length(rest) - 3)
    if (index(rest, "{{") || index(rest, "}}") || trim(rest) == "") return 0
    depth = 0
    for (i = 1; i <= length(rest); i++) {
        ch = substr(rest, i, 1)
        if (ch == "(") depth++
        else if (ch == ")") { depth--; if (depth < 0) return 0 }
        else if (ch == "|" && substr(rest, i + 1, 1) == "|" && depth == 0) return 0
    }
    return depth == 0
}
/^[ \t]*#/ { next }
/^[^ ]/ { top = $0; sub(/:.*/, "", top); job = ""; inperm = 0 }
/^on:/ { inon = 1; next }
/^[^ ]/ { inon = 0 }
inon && /^  [^ ]/ { v = trim($0); sub(/:.*/, "", v); print "on " v }
top == "permissions" && /^permissions:/ { v = trim(substr($0, 13)); if (v != "") print "top " v; next }
top == "permissions" && /^  [^ ]/ { print "top " trim($0) }
top == "jobs" && /^  [a-z][a-z0-9_-]*:[ ]*$/ { job = trim($0); sub(/:$/, "", job); inperm = 0; next }
job != "" && /^    [^ ]/ { inperm = 0 }
job != "" && /^    permissions:/ { inperm = (trim(substr($0, index($0, ":") + 1)) == "") }
/write-all/ { print "write-all " (job != "" ? job : top) }
/id-token/ { print "id-token " (job != "" ? job : top) " " (inperm && /^      id-token: write$/ ? "block" : "other") }
/attestations["']?[ \t]*:/ { print "attestations " (job != "" ? job : top) " " (inperm && /^      attestations: write$/ ? "block" : "other") }
job != "" && /^    if:/ { e = trim(substr($0, index($0, ":") + 1)); print "if " job " " guarded(e) }
AWK
id_token_breach() {
    facts=$(awk -f "$work/permissions.awk" "$1")
    [ "$(printf '%s\n' "$facts" | sed -n 's/^top //p')" = "contents: read" ] ||
        { echo "the top-level permissions are not contents: read alone"; return 1; }
    [ "$(printf '%s\n' "$facts" | sed -n 's/^on //p' | sort | tr '\n' ' ')" = "pull_request push workflow_dispatch " ] ||
        { echo "the workflow runs on other events than push, workflow_dispatch and pull_request"; return 1; }
    ! printf '%s\n' "$facts" | grep -q '^write-all ' || { echo "a permission is write-all"; return 1; }
    [ "$(printf '%s\n' "$facts" | grep '^id-token ' | sort | tr '\n' ';')" = "id-token image-manifest block;id-token sign block;" ] ||
        { echo "id-token is not write on sign and image-manifest alone: $(printf '%s\n' "$facts" | grep '^id-token ' | tr '\n' ';')"; return 1; }
    [ "$(printf '%s\n' "$facts" | grep '^attestations ' | tr '\n' ';')" = "attestations sign block;" ] ||
        { echo "attestations is not write on sign alone: $(printf '%s\n' "$facts" | grep '^attestations ' | tr '\n' ';')"; return 1; }
    for holder in sign image-manifest; do
        [ "$(printf '%s\n' "$facts" | grep "^if $holder " | tr '\n' ';')" = "if $holder 1;" ] ||
            { echo "the $holder job, which holds id-token: write, has no single if of an accepted guard form"; return 1; }
    done
}
cases=$((cases + 1))
why=$(id_token_breach "$workflow") || fail "adversary_id_token_only_on_signing_jobs: release.yml: $why"
# Each copy breaks one rule; the check must refuse every one.
mutant() {
    awk -v kind="$1" '
        /^  [a-z][a-z0-9_-]*:$/ { job = $1 }
        /^[^ ]/ { job = "" }
        kind == "if-removed" && job == "sign:" && /^    if:/ { next }
        kind == "or-true" && job == "sign:" && /^    if:/ { sub(/ }}$/, " || true }}") }
        kind == "or-true-outside" && job == "sign:" && /^    if:/ { $0 = $0 " || true" }
        kind == "unparenthesised-or" && job == "image-manifest:" && /^    if:/ { sub(/ }}$/, " || github.event_name == '"'"'pull_request'"'"' }}") }
        kind == "folded-if" && job == "sign:" && /^    if:/ { print "    if: >-"; sub(/^    if:/, "     "); }
        { print }
        kind == "top" && $0 == "permissions:" { print "  id-token: write" }
        kind == "build" && $0 == "  build:" { print "    permissions:"; print "      contents: read"; print "      id-token: write" }
        kind == "write-all" && $0 == "  ui:" { print "    permissions: write-all" }
        kind == "flow" && $0 == "  gate:" { print "    permissions: { contents: read, id-token: write }" }
        kind == "attest-publish" && $0 == "      contents: write" { print "      attestations: write" }
        kind == "target" && $0 == "  pull_request:" { print "  pull_request_target:" }
    ' "$workflow" >"$work/mutant.yml"
}
for kind in top build if-removed or-true or-true-outside unparenthesised-or folded-if write-all flow attest-publish target; do
    mutant "$kind"
    cases=$((cases + 1))
    if cmp -s "$workflow" "$work/mutant.yml"; then
        fail "adversary_id_token_only_on_signing_jobs: the copy '$kind' is release.yml unchanged"
    elif id_token_breach "$work/mutant.yml" >/dev/null; then
        fail "adversary_id_token_only_on_signing_jobs: a copy of release.yml with '$kind' passes"
    fi
done

# adversary_image_signed_by_manifest_digest: image-manifest's signing step signs
# <image>@sha256:<digest> read from IMAGES, never a tag, and signs nothing for an IMAGES line that
# is not the release's image pinned by digest.
step_script image-manifest "Sign the manifest list" "$work/steps/sign-image.sh"
image_signs() {
    rm -rf "$work/signimage"
    mkdir -p "$work/signimage/images"
    printf '%s\n' "$2" >"$work/signimage/images/IMAGES"
    rm -f "$work/rec/cosign-log"
    (cd "$work/signimage" && PATH="$work/sigtools:$PATH" IMAGE=ghcr.io/beyond10x/spindle VERSION=0.26.0 \
        GITHUB_STEP_SUMMARY=$work/signimage/summary bash "$1" >"$work/signimage/out" 2>&1)
}
# Passes when the step signs exactly the image by the digest of a good IMAGES line and nothing
# for a bad one.
image_signed_by_digest() {
    image_signs "$1" "ghcr.io/beyond10x/spindle:0.26.0@sha256:$hex" || { echo "the step fails: $(cat "$work/signimage/out")"; return 1; }
    [ "$(cat "$work/rec/cosign-log" 2>/dev/null)" = "sign --yes ghcr.io/beyond10x/spindle@sha256:$hex" ] ||
        { echo "cosign was called as '$(tr '\n' ';' <"$work/rec/cosign-log" 2>/dev/null)', not sign --yes ghcr.io/beyond10x/spindle@sha256:$hex"; return 1; }
    for line in "ghcr.io/beyond10x/spindle:0.26.0" "ghcr.io/beyond10x/spindle:0.25.0@sha256:$hex" \
        "ghcr.io/beyond10x/other:0.26.0@sha256:$hex" "ghcr.io/beyond10x/spindle:0.26.0@sha256:${hex%?}" ""; do
        if image_signs "$1" "$line" || [ -s "$work/rec/cosign-log" ]; then
            echo "the step signs for the IMAGES line '$line': $(tr '\n' ';' <"$work/rec/cosign-log" 2>/dev/null)"
            return 1
        fi
    done
}
cases=$((cases + 1))
why=$(image_signed_by_digest "$work/steps/sign-image.sh") || fail "adversary_image_signed_by_manifest_digest: $why"
sed 's|^cosign sign --yes .*|cosign sign --yes "${IMAGE}:${VERSION}"|' "$work/steps/sign-image.sh" >"$work/steps/sign-image-tag.sh"
cases=$((cases + 1))
if cmp -s "$work/steps/sign-image.sh" "$work/steps/sign-image-tag.sh"; then
    fail "adversary_image_signed_by_manifest_digest: the step has no line 'cosign sign --yes ...' to turn into a tag"
elif image_signed_by_digest "$work/steps/sign-image-tag.sh" >/dev/null; then
    fail "adversary_image_signed_by_manifest_digest: a step that signs <image>:<version> passes"
fi

# adversary_pinned_tools_are_checked: sign, image-manifest and publish each fetch cosign v3.1.6 in
# one identical step that checks it with sha256sum --check --strict against the pinned sha256
# before it runs it, so a download with one byte changed stops the step before cosign runs; no
# other step fetches cosign; every uses: in release.yml, actions/attest-build-provenance among
# them, names a 40-hex commit.
cosign_sha256=318a99ade4156a0c4113aa983b46c0a0830fa7896a096c63121ee47b610a94c6
attest_commit=4d101475d8b20a2381f78447822ac1eab6504dd8
# A step's whole text, from its `- name:` line to the next step or job.
step_text() {
    job "$1" | awk -v want="      - name: $2" '
        index($0, want) == 1 { found = 1; print; next }
        found && (/^      - / || /^      #/ || /^  [a-z]/) { exit }
        found { print }
    '
}
for name in sign image-manifest publish; do
    step_text "$name" "Install cosign" >"$work/steps/cosign-$name.yml"
done
cases=$((cases + 1))
[ -s "$work/steps/cosign-sign.yml" ] && cmp -s "$work/steps/cosign-sign.yml" "$work/steps/cosign-image-manifest.yml" &&
    cmp -s "$work/steps/cosign-sign.yml" "$work/steps/cosign-publish.yml" ||
    fail "adversary_pinned_tools_are_checked: sign, image-manifest and publish do not each hold the same step 'Install cosign'"
cases=$((cases + 1))
grep -Fqx "          COSIGN_VERSION: '3.1.6'" "$work/steps/cosign-sign.yml" &&
    grep -Fqx "          COSIGN_SHA256: $cosign_sha256" "$work/steps/cosign-sign.yml" ||
    fail "adversary_pinned_tools_are_checked: the step 'Install cosign' does not pin cosign 3.1.6 at $cosign_sha256: $(cat "$work/steps/cosign-sign.yml")"
cases=$((cases + 1))
[ "$(grep -c 'sigstore/cosign/releases' "$workflow")" -eq 3 ] && [ "$(grep -c 'sigstore/cosign/releases' "$work/steps/cosign-sign.yml")" -eq 1 ] ||
    fail "adversary_pinned_tools_are_checked: a step other than the three 'Install cosign' fetches cosign"
for name in $(sed -n '/^jobs:$/,$p' "$workflow" | sed -n 's/^  \([a-z][a-z0-9-]*\):$/\1/p'); do
    if job "$name" | grep -Eq '^ *cosign (sign|verify)|[ (]cosign (sign|verify)'; then
        cases=$((cases + 1))
        [ -s "$work/steps/cosign-$name.yml" ] ||
            fail "adversary_pinned_tools_are_checked: the $name job runs cosign without the step 'Install cosign'"
    fi
done
cases=$((cases + 1))
unpinned=$(grep -E '^ *(- )?uses: ' "$workflow" | grep -Ev '@[0-9a-f]{40}( # .*)?$' || true)
[ -z "$unpinned" ] || fail "adversary_pinned_tools_are_checked: a uses: is not pinned by a 40-hex commit: $unpinned"
cases=$((cases + 1))
attests=$(grep -E '^ *(- )?uses: actions/attest-build-provenance@' "$workflow" || true)
[ "$(printf '%s\n' "$attests" | grep -c "uses: actions/attest-build-provenance@$attest_commit # v4.2.2$")" -eq 2 ] &&
    [ "$(printf '%s\n' "$attests" | grep -c '')" -eq 2 ] ||
    fail "adversary_pinned_tools_are_checked: actions/attest-build-provenance is not used twice at $attest_commit (v4.2.2): $attests"
# The step's run block on a curl stub that writes cosign-download, with the pin set to the sha256
# of the stub's download, then with one byte of the download changed.
step_script sign "Install cosign" "$work/steps/install-cosign.sh"
mkdir -p "$work/fetchtools"
printf '#!%s\nrec=%s\n' "$shell" "$work/rec" >"$work/fetchtools/curl"
cat >>"$work/fetchtools/curl" <<'STUB'
printf '%s\n' "$*" >>"$rec/fetch-log"
out=""
while [ $# -gt 0 ]; do
    case $1 in --output) out=$2; shift 2 ;; *) shift ;; esac
done
[ -n "$out" ] || exit 2
cp "$rec/cosign-download" "$out"
STUB
chmod 755 "$work/fetchtools/curl"
printf '#!/bin/sh\necho ran >>"%s/cosign-ran"\n' "$work/rec" >"$work/rec/cosign-download"
pin=$(sha256sum <"$work/rec/cosign-download" | cut -d ' ' -f 1)
install_cosign() {
    rm -rf "$work/install"
    mkdir -p "$work/install"
    : >"$work/install/path"
    rm -f "$work/rec/cosign-ran" "$work/rec/fetch-log"
    (PATH="$work/fetchtools:$PATH" RUNNER_TEMP=$work/install GITHUB_PATH=$work/install/path COSIGN_VERSION=3.1.6 \
        COSIGN_SHA256=$pin bash "$work/steps/install-cosign.sh" >"$work/install/out" 2>&1)
}
cases=$((cases + 1))
if install_cosign; then
    [ -s "$work/rec/cosign-ran" ] && [ -s "$work/install/path" ] &&
        grep -Fq 'https://github.com/sigstore/cosign/releases/download/v3.1.6/cosign-linux-amd64' "$work/rec/fetch-log" ||
        fail "adversary_pinned_tools_are_checked: the step with a matching download does not fetch cosign 3.1.6, put it on PATH and run it"
else
    fail "adversary_pinned_tools_are_checked: the step fails on a download that matches its pin: $(cat "$work/install/out")"
fi
sed 's/ran/rAn/' "$work/rec/cosign-download" >"$work/rec/cosign-changed"
mv "$work/rec/cosign-changed" "$work/rec/cosign-download"
cases=$((cases + 1))
! install_cosign && [ ! -e "$work/rec/cosign-ran" ] && [ ! -s "$work/install/path" ] ||
    fail "adversary_pinned_tools_are_checked: the step runs or installs a cosign download with one byte changed"

# adversary_backfill_verifies_at_main: publish's verification step, on the assets downloaded back
# from the page, passes cosign the release workflow at refs/heads/main as the identity on a
# backfill (workflow_dispatch) and at refs/tags/<tag> on a tag push, and fails the run when a
# check fails. Its copy of one archive with one byte changed must fail both cosign and gh.
step_script publish "Verify the signatures" "$work/steps/verify.sh"
tagged=$(printf '%040d' 7)
dispatched=$(printf '%040d' 8)
# A signed release of 0.26.0 on the page: the bundles of sign's step, every asset attested, and
# the image's digest from IMAGES.
on_page() {
    signed "$1" >/dev/null 2>&1 || return 1
    rm -rf "$work/rec/page"
    mkdir -p "$work/rec/page"
    cp "$work/dist/"* "$work/rec/page/"
    (cd "$work/rec/page" && for asset in $(assets_of "$1"); do sha256sum <"$asset" | cut -d ' ' -f 1; done) >"$work/rec/attested"
    printf '%s\n' "$hex" >>"$work/rec/attested"
}
verify_run() {
    rm -rf "$work/verify"
    mkdir -p "$work/verify"
    rm -f "$work/rec/cosign-log"
    (cd "$work/verify" && PATH="$work/ghtools:$work/sigtools:$PATH" RUNNER_TEMP=$work/verify GH_REPO=beyond10x/spindle \
        TAG=0.26.0 COMMIT=$tagged GITHUB_SHA=$2 GITHUB_EVENT_NAME=$3 IMAGES_EXPECTED=$4 IMAGE=ghcr.io/beyond10x/spindle \
        GITHUB_STEP_SUMMARY=$work/verify/summary bash "$1" >"$work/verify/out" 2>&1)
}
# Passes when the step verifies a backfill at refs/heads/main and a tag push at the tag's ref,
# every cosign check naming that identity, and refuses an identity it is not given.
verifies_at_ref() {
    step=$1
    for row in "workflow_dispatch $dispatched refs/heads/main" "push $tagged refs/tags/0.26.0"; do
        set -- $row
        echo "$2" >"$work/rec/attested-commit"
        verify_run "$step" "$2" "$1" true || { echo "the step fails on a $1 run: $(cat "$work/verify/out")"; return 1; }
        identity="--certificate-identity https://github.com/beyond10x/spindle/.github/workflows/release.yml@$3 --certificate-oidc-issuer https://token.actions.githubusercontent.com"
        calls=$(grep -E '^verify(-blob)? ' "$work/rec/cosign-log" 2>/dev/null || true)
        [ "$(grep -cE '^verify(-blob)? ' "$work/rec/cosign-log" 2>/dev/null || true)" = 10 ] ||
            { echo "a $1 run calls cosign verify $(grep -cE '^verify(-blob)? ' "$work/rec/cosign-log" 2>/dev/null || true) times, not 8 assets, the changed archive and the image: $(printf '%s' "$calls" | tr '\n' ';')"; return 1; }
        [ -z "$(printf '%s\n' "$calls" | grep -Fv -- "$identity")" ] ||
            { echo "a $1 run passes cosign another identity than $3: $(printf '%s' "$calls" | tr '\n' ';')"; return 1; }
    done
}
cases=$((cases + 1))
if ! on_page true; then
    fail "adversary_backfill_verifies_at_main: no signed release to put on the page: $(cat "$work/sign.out")"
else
    why=$(verifies_at_ref "$work/steps/verify.sh") || fail "adversary_backfill_verifies_at_main: $why"
    for swap in 's|refs/heads/main|refs/tags/${TAG}|' 's|refs/tags/${TAG}|refs/heads/main|'; do
        sed "$swap" "$work/steps/verify.sh" >"$work/steps/verify-swapped.sh"
        cases=$((cases + 1))
        if cmp -s "$work/steps/verify.sh" "$work/steps/verify-swapped.sh"; then
            fail "adversary_backfill_verifies_at_main: the step has no ref to swap with '$swap'"
        elif verifies_at_ref "$work/steps/verify-swapped.sh" >/dev/null; then
            fail "adversary_backfill_verifies_at_main: a step that passes the other ref ('$swap') passes"
        fi
    done
    # The run fails when a check fails: an attestation naming another commit, a bundle that does
    # not match its asset, a check on the changed archive that passes, an event with no identity.
    echo "$dispatched" >"$work/rec/attested-commit"
    cases=$((cases + 1))
    ! verify_run "$work/steps/verify.sh" "$tagged" push true ||
        fail "adversary_backfill_verifies_at_main: a tag push passes an attestation that names another commit"
    echo "$tagged" >"$work/rec/attested-commit"
    cp "$work/rec/page/SHA256SUMS.sigstore.json" "$work/rec/page/b10x-spindle-0.26.0.cdx.json.sigstore.json" 2>/dev/null || true
    cases=$((cases + 1))
    ! verify_run "$work/steps/verify.sh" "$tagged" push true ||
        fail "adversary_backfill_verifies_at_main: a release passes whose SBOM bundle does not match the SBOM"
    on_page true >/dev/null 2>&1 || true
    # cosign and gh that pass anything: the changed archive must then fail the step.
    mkdir -p "$work/lenient"
    printf '#!%s\necho "Verified OK"\n' "$shell" >"$work/lenient/cosign"
    printf '#!%s\nrec=%s\nreal=%s\n' "$shell" "$work/rec" "$work/ghtools/gh" >"$work/lenient/gh"
    cat >>"$work/lenient/gh" <<'STUB'
case "$1 $2" in
    "attestation verify")
        printf '[{"verificationResult":{"signature":{"certificate":{"sourceRepositoryDigest":"%s"}}}}]\n' "$(cat "$rec/attested-commit")"
        ;;
    *) exec "$real" "$@" ;;
esac
STUB
    chmod 755 "$work/lenient/cosign" "$work/lenient/gh"
    cases=$((cases + 1))
    ! (cd "$work/verify" && PATH="$work/lenient:$PATH" RUNNER_TEMP=$work/verify GH_REPO=beyond10x/spindle TAG=0.26.0 \
        COMMIT=$tagged GITHUB_SHA=$tagged GITHUB_EVENT_NAME=push IMAGES_EXPECTED=true IMAGE=ghcr.io/beyond10x/spindle \
        GITHUB_STEP_SUMMARY=$work/verify/summary bash "$work/steps/verify.sh" >"$work/verify/out" 2>&1) ||
        fail "adversary_backfill_verifies_at_main: the step passes though cosign and gh verify the archive with one byte changed"
    for event in pull_request schedule; do
        cases=$((cases + 1))
        ! verify_run "$work/steps/verify.sh" "$tagged" "$event" true ||
            fail "adversary_backfill_verifies_at_main: the step verifies a $event run against some identity"
    done
    cases=$((cases + 1))
    verify_run "$work/steps/verify.sh" "$tagged" push true || true
    grep -Fq '$ cosign verify-blob --bundle SHA256SUMS.sigstore.json' "$work/verify/summary" &&
        grep -Fq '$ gh attestation verify oci://ghcr.io/beyond10x/spindle:0.26.0@sha256:' "$work/verify/summary" ||
        fail "adversary_backfill_verifies_at_main: the step summary does not hold each command: $(head -c 600 "$work/verify/summary")"
fi

# --- adversary_w34_u1 ------------------------------------------------------------------------
# Adversarial cases for story:release-artifacts-signed (wave 34, unit U1), added after the unit's
# own cases were green. Each case is named in its fail message.
w34u1=$work/adversary-w34-u1
mkdir -p "$w34u1/latefail" "$w34u1/twice"

# adversary_w34_u1_sign_fails_when_cosign_fails_late: a cosign sign-blob that writes the bundle
# and then exits non-zero (as a signing that fails after the bundle is on disk) fails sign's
# signing step. The step's `test -s` after cosign does not catch this; only the exit status does,
# so a copy of the step with `|| true` after sign-blob must be caught.
printf '#!%s\nreal=%s\n' "$shell" "$work/sigtools/cosign" >"$w34u1/latefail/cosign"
cat >>"$w34u1/latefail/cosign" <<'STUB'
status=0
"$real" "$@" || status=$?
for last in "$@"; do :; done
if [ "$1" = sign-blob ] && [ "$(basename "$last")" = b10x-spindle-0.26.0.cdx.json ]; then
    echo "stub cosign: bundle written, then the signing failed" >&2
    exit 1
fi
exit "$status"
STUB
chmod 755 "$w34u1/latefail/cosign"
# Passes when the step fails on a cosign that fails late on the SBOM.
w34u1_sign_refuses_late_failure() {
    sums true IMAGES >/dev/null 2>&1 || { echo "no checked artifacts to sign: $(cat "$work/sums.out")"; return 1; }
    rm -rf "$w34u1/signrun"
    mkdir -p "$w34u1/signrun"
    if (cd "$work/dist" && PATH="$w34u1/latefail:$work/sigtools:$PATH" RUNNER_TEMP=$w34u1/signrun \
        GITHUB_STEP_SUMMARY=$w34u1/signrun/summary bash "$1" >"$w34u1/signrun/out" 2>&1); then
        echo "the step passes though cosign sign-blob exits 1 on the SBOM after writing its bundle"
        return 1
    fi
}
cases=$((cases + 1))
why=$(w34u1_sign_refuses_late_failure "$work/steps/sign.sh") ||
    fail "adversary_w34_u1_sign_fails_when_cosign_fails_late: $why"
sed 's/^\( *cosign sign-blob .*\)$/\1 || true/' "$work/steps/sign.sh" >"$w34u1/sign-or-true.sh"
cases=$((cases + 1))
if cmp -s "$work/steps/sign.sh" "$w34u1/sign-or-true.sh"; then
    fail "adversary_w34_u1_sign_fails_when_cosign_fails_late: the step has no 'cosign sign-blob' line to append || true to"
elif w34u1_sign_refuses_late_failure "$w34u1/sign-or-true.sh" >/dev/null; then
    fail "adversary_w34_u1_sign_fails_when_cosign_fails_late: a step with || true after cosign sign-blob passes the case"
fi

# adversary_w34_u1_guard_quoted_parens: the accepted guard forms allow no || outside parentheses.
# A parenthesis inside a string literal is not a parenthesis of the expression, so
#   ${{ github.event_name != 'pull_request' && '(' || true || ')' }}
# is true on a pull_request event (false && '(' || true || ')' is true) and must be refused.
awk '
    /^  [a-z][a-z0-9_-]*:$/ { job = $1 }
    /^[^ ]/ { job = "" }
    job == "sign:" && /^    if:/ { print "    if: ${{ github.event_name != '"'"'pull_request'"'"' && '"'"'('"'"' || true || '"'"')'"'"' }}"; next }
    { print }
' "$workflow" >"$w34u1/quoted-parens.yml"
cases=$((cases + 1))
if cmp -s "$workflow" "$w34u1/quoted-parens.yml"; then
    fail "adversary_w34_u1_guard_quoted_parens: the copy is release.yml unchanged"
elif id_token_breach "$w34u1/quoted-parens.yml" >/dev/null; then
    fail "adversary_w34_u1_guard_quoted_parens: a copy of release.yml whose sign job runs on a pull request (if: $(sed -n '/^  sign:$/,/^    if:/s/^    if: //p' "$w34u1/quoted-parens.yml")) passes adversary_id_token_only_on_signing_jobs"
fi

# adversary_w34_u1_backfill_only_from_main: a backfill is signed by the workflow at
# refs/heads/main (D6 A), and publish verifies it only at that ref, after the upload, which is
# never replaced. A workflow_dispatch run from any other ref (a branch or a tag picked in "Use
# workflow from") must stop before anything is signed: provenance, or both sign and
# image-manifest, compare github.ref (GITHUB_REF) with refs/heads/main.
w34u1_ref_guard() {
    job "$1" | grep -Eq '(github\.ref|GITHUB_REF)[^_a-z].*refs/heads/main|refs/heads/main.*(github\.ref|GITHUB_REF)([^_a-z]|$)'
}
cases=$((cases + 1))
w34u1_ref_guard provenance || { w34u1_ref_guard sign && w34u1_ref_guard image-manifest; } ||
    fail "adversary_w34_u1_backfill_only_from_main: nothing before sign or image-manifest refuses a workflow_dispatch run whose ref is not refs/heads/main; its assets are signed as release.yml@<that ref>, uploaded, and only then fail publish's verification"

# adversary_w34_u1_backfill_keeps_the_tag_runs_attestation: a backfill of a release whose tag
# run attested the image and then stopped before the upload (the image tag is then left as it
# is, so IMAGES names the same digest) finds two attestations of the image: the tag run's, at the
# tagged commit, and its own, at main's. D6 A accepts either ref, so the step passes.
tagged=$(printf '%040d' 7)
dispatched=$(printf '%040d' 8)
printf '#!%s\nreal=%s\n' "$shell" "$work/ghtools/gh" >"$w34u1/twice/gh"
printf 'tagged=%s\ndispatched=%s\n' "$tagged" "$dispatched" >>"$w34u1/twice/gh"
cat >>"$w34u1/twice/gh" <<'STUB'
case "$1 $2 $3" in
    "attestation verify oci://"*)
        "$real" "$@" >/dev/null || exit $?
        for commit in "$tagged" "$dispatched"; do
            printf '{"verificationResult":{"signature":{"certificate":{"buildSignerURI":"stub","sourceRepositoryDigest":"%s"}}}}\n' "$commit"
        done | sed -n '1s/^/[/; $!s/$/,/; $s/$/]/; p'
        ;;
    *) exec "$real" "$@" ;;
esac
STUB
chmod 755 "$w34u1/twice/gh"
cases=$((cases + 1))
if ! on_page true; then
    fail "adversary_w34_u1_backfill_keeps_the_tag_runs_attestation: no signed release to put on the page: $(cat "$work/sign.out")"
else
    echo "$dispatched" >"$work/rec/attested-commit"
    rm -rf "$work/verify"
    mkdir -p "$work/verify"
    (cd "$work/verify" && PATH="$w34u1/twice:$work/sigtools:$PATH" RUNNER_TEMP=$work/verify GH_REPO=beyond10x/spindle \
        TAG=0.26.0 COMMIT=$tagged GITHUB_SHA=$dispatched GITHUB_EVENT_NAME=workflow_dispatch IMAGES_EXPECTED=true \
        IMAGE=ghcr.io/beyond10x/spindle GITHUB_STEP_SUMMARY=$work/verify/summary bash "$work/steps/verify.sh" \
        >"$work/verify/out" 2>&1) ||
        fail "adversary_w34_u1_backfill_keeps_the_tag_runs_attestation: a backfill fails on the image's attestation from the tag run: $(grep -A3 'oci://' "$work/verify/summary" | tr '\n' ' ')"
fi

echo "release-linux-binary-adversary: $cases cases, $failures failed"
[ "$failures" -eq 0 ]
