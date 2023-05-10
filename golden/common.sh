ENTRY_FUNC="main"

if ! command -v qsym 1>/dev/null; then
	echo "Error: Couldn't find qsym in \$PATH'" 1>&2
	exit 1
fi
