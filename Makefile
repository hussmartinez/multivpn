PREFIX ?= /usr/local
BINDIR := $(PREFIX)/bin
SERVICE := multivpn.service
BINARIES := mvpn mvpn-daemon mvpn-tui mvpn-tray

.PHONY: build test install uninstall clean enable disable

build:
	cargo build --release

test:
	cargo test --workspace

install: build
	install -Dm755 target/release/mvpn        $(DESTDIR)$(BINDIR)/mvpn
	install -Dm755 target/release/mvpn-daemon  $(DESTDIR)$(BINDIR)/mvpn-daemon
	install -Dm755 target/release/mvpn-tui     $(DESTDIR)$(BINDIR)/mvpn-tui
	install -Dm755 target/release/mvpn-tray    $(DESTDIR)$(BINDIR)/mvpn-tray
	install -Dm644 crates/mvpn-daemon/$(SERVICE) $(DESTDIR)/etc/systemd/system/$(SERVICE)
	mkdir -p $(HOME)/.config/multivpn
	systemctl daemon-reload

uninstall:
	-systemctl stop $(SERVICE)
	-systemctl disable $(SERVICE)
	$(foreach bin,$(BINARIES),rm -f $(DESTDIR)$(BINDIR)/$(bin);)
	rm -f $(DESTDIR)/etc/systemd/system/$(SERVICE)
	systemctl daemon-reload

clean:
	cargo clean

enable:
	systemctl enable --now $(SERVICE)

disable:
	systemctl disable --now $(SERVICE)
