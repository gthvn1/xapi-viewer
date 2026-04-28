.PHONY: run

run:
	cargo run -- samples/xensource.log 2>/tmp/debug.log
