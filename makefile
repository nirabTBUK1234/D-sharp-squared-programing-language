build:
	cargo build
	cp target/debug/NewProgrammingLanguageWithRust Runner
run:
	./Runner "$(word 2,$(MAKECMDGOALS))"
all: build run