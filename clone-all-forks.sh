#!/bin/bash
set -ex

cd "$(dirname "$0")"/..

git clone git@github.com:Adjective-Object/schematic.git
git clone git@github.com:Adjective-Object.git
git clone git@github.com:Adjective-Object/proto.git
git clone git@github.com:Adjective-Object/starbase.git
git clone git@github.com:Adjective-Object/netrc.git
