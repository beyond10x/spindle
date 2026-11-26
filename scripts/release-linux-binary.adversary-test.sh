#!/bin/sh
# Adversarial cases for scripts/release-linux-binary.sh and the target lists of
# .github/workflows/release.yml (story:release-linux-glibc-floor-and-musl). POSIX sh; run as
# `sh scripts/release-linux-binary.adversary-test.sh`. The checked tools (objdump, file, readelf,
# docker) are stubs that print recorded output, so no case builds, pulls or runs an image; the
# stubs sit in a PATH of their own, so a case can also take a tool away.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
subject=${RELEASE_LINUX_BINARY:-$root/scripts/release-linux-binary.sh}
workflow=${RELEASE_WORKFLOW:-$root/.github/workflows/release.yml}
work=$root/target/release-linux-binary-adversary
rm -rf "$work"
mkdir -p "$work/rec" "$work/tools"

failures=0
cases=0
shell=$(command -v sh)

fail() {
    echo "release-linux-binary-adversary: FAIL: $*" >&2
    failures=$((failures + 1))
}

# The plain tools the subject calls, without any of the checked ones.
for tool in grep sed sort tail head dirname basename cat; do
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
stub docker "printf '%s\n' \"\$*\" >'$work/rec/docker-args'; cat '$work/rec/docker-out'; exit \$(cat '$work/rec/docker-status')"

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
        refuse) [ "$got" -ne 0 ] || fail "$label: passed: $(cat "$work/out")" ;;
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

# --- the workflow's target lists -------------------------------------------------------------

matrix=$(sed -n 's/^ *- target: *//p' "$workflow" | sort)
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

echo "release-linux-binary-adversary: $cases cases, $failures failed"
[ "$failures" -eq 0 ]
