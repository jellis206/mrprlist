# mrprlist -- build & install following the GNU PREFIX/DESTDIR convention.
#
#   make build                       # compile release binary
#   make install                     # install to ~/.local (PREFIX)
#   make install PREFIX=/usr/local   # or anywhere
#
# The dotfiles tool-builder invokes `make install PREFIX="$HOME/.local"`.

PREFIX  ?= $(HOME)/.local
DESTDIR ?=
CARGO   ?= cargo

BINDIR  := $(DESTDIR)$(PREFIX)/bin

.PHONY: all build install uninstall clean

all: build

build:
	$(CARGO) build --release --locked

install: build
	install -d "$(BINDIR)"
	install -m 0755 target/release/mrprlist "$(BINDIR)/mrprlist"
	@echo "installed mrprlist -> $(BINDIR)/mrprlist"

uninstall:
	rm -f "$(BINDIR)/mrprlist"

clean:
	$(CARGO) clean
