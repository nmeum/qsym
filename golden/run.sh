#!/bin/sh

cd "$(dirname "$0")"
. common.sh

ret=0
for test in *; do
	[ -d "${test}" ] || continue

	name="${test##*/}"
	printf "Running test case '%s': " "${name}"

	qsym "${test}"/input.qbe "${ENTRY_FUNC}" 2>&1 | \
		cmp - "${test}/expected" 2>/dev/null 1>&2
	if [ $? -ne 0 ]; then
		echo FAIL
		ret=1
	else
		echo OK
	fi
done

exit "${ret}"
