.PHONY: run

run:
	cargo run -- samples/xensource.log --db samples/state.db 2>/tmp/debug.log
