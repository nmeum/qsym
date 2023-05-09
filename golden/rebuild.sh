#!/bin/sh
set -e

cd "$(dirname "$0")"
. common.sh

if ! command -v qbe 1>/dev/null; then
	echo "Error: Couldn't find qbe in \$PATH'" 1>&2
	exit 1
fi

for test in *; do
	[ -d "${test}" ] || continue

	# Ensure that qbe(1) considers the input to be syntactically valid.
	qbe "${test}"/input.qbe 1>/dev/null 2>&1 || (
		echo "File '${test}/input.qbe' is not valid QBE IL"
		exit 1
	)

	qsym "${test}"/input.qbe "${ENTRY_FUNC}" \
		1>"${test}"/expected 2>&1
done
