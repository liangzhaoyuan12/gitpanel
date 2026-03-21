PREFIX ?= $(HOME)/.local
BINDIR = $(PREFIX)/bin
DATADIR = $(PREFIX)/share
APP_ID = io.gitlab.ilshat_apps.gitpulsar

.PHONY: build install uninstall help

build:
	cargo build --release

install: build
	install -Dm755 target/release/gitpulsar-gtk $(BINDIR)/gitpulsar-gtk
	install -Dm644 data/$(APP_ID).desktop $(DATADIR)/applications/$(APP_ID).desktop
	sed -i 's|Icon=$(APP_ID)|Icon=$(DATADIR)/icons/hicolor/scalable/apps/$(APP_ID).svg|' $(DATADIR)/applications/$(APP_ID).desktop
	install -Dm644 data/$(APP_ID).metainfo.xml $(DATADIR)/metainfo/$(APP_ID).metainfo.xml
	install -Dm644 data/icons/hicolor/scalable/apps/$(APP_ID).svg $(DATADIR)/icons/hicolor/scalable/apps/$(APP_ID).svg
	@for icon in data/icons/hicolor/scalable/actions/*.svg; do \
		install -Dm644 "$$icon" "$(DATADIR)/icons/hicolor/scalable/actions/$$(basename $$icon)"; \
	done
	@echo "Installed to $(PREFIX). Make sure $(BINDIR) is in your PATH."

uninstall:
	rm -f $(BINDIR)/gitpulsar-gtk
	rm -f $(DATADIR)/applications/$(APP_ID).desktop
	rm -f $(DATADIR)/metainfo/$(APP_ID).metainfo.xml
	rm -f $(DATADIR)/icons/hicolor/scalable/apps/$(APP_ID).svg
	rm -f $(DATADIR)/icons/hicolor/scalable/actions/branch-*.svg
	rm -f $(DATADIR)/icons/hicolor/scalable/actions/commit-*.svg
	rm -f $(DATADIR)/icons/hicolor/scalable/actions/history-*.svg
	rm -f $(DATADIR)/icons/hicolor/scalable/actions/pull-request-*.svg
	rm -f $(DATADIR)/icons/hicolor/scalable/actions/tag-*.svg

help:
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  build      Build release binary (cargo build --release)"
	@echo "  install    Build and install to PREFIX (default: ~/.local)"
	@echo "  uninstall  Remove installed files"
	@echo "  help       Show this help"
	@echo ""
	@echo "Variables:"
	@echo "  PREFIX     Installation prefix (default: $(HOME)/.local)"
