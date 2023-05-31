#!/usr/bin/awk -f

BEGIN {
	in_halt = 0
}

/^	$/ {
	in_halt = 0
}

/^Halting executing$/ {
	in_halt = 1
}

/^	main:.1 -> / {
	if (in_halt)
		print($3)
}
