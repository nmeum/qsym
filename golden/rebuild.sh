#!/bin/sh
set -e

cd "$(dirname "$0")"
. common.sh

for test in *; do
	[ -d "${test}" ] || continue

	qsym "${test}"/input.qbe "${ENTRY_FUNC}" \
		1>"${test}"/expected 2>&1
done
