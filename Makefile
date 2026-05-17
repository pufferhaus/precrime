# PRECRIME build + deploy targets.
#
# Build runs on macOS via `cross` (Docker/colima) targeting aarch64.
# Override hosts per invocation:
#   make deploy-report REPORT_HOST=192.168.50.10
#   make logs-precog   PRECOG_HOST=precog-02-cctv-door.local

TARGET      := aarch64-unknown-linux-gnu
RELEASE_DIR := target/$(TARGET)/release

PRECOG_HOST ?= precog-01.local
PRECOG_USER ?= precog
REPORT_HOST ?= report.local
REPORT_USER ?= pi

.PHONY: help check fmt clippy test \
        build-precog build-report build-all clean-cross \
        deploy-report install-report logs-report restart-report \
        deploy-precog install-precog logs-precog restart-precog

help:
	@echo "Build (cross-compile from macOS to aarch64):"
	@echo "  make build-precog | build-report | build-all"
	@echo "  make clean-cross"
	@echo ""
	@echo "Deploy (build + rsync binary + restart systemd):"
	@echo "  make deploy-precog PRECOG_HOST=...  PRECOG_USER=..."
	@echo "  make deploy-report REPORT_HOST=...  REPORT_USER=..."
	@echo ""
	@echo "Iterate:"
	@echo "  make logs-precog | logs-report"
	@echo "  make restart-precog | restart-report"
	@echo ""
	@echo "First-time provision (apt + systemd unit, run once per Pi):"
	@echo "  make install-precog | install-report"

# --- Local workspace ---

check:
	cargo check

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings

test:
	cargo test

# ---- Cross-compile from macOS to aarch64 (Raspberry Pi 5) ----

build-precog:
	cross build --release --target $(TARGET) --package precog

build-report:
	cross build --release --target $(TARGET) --package report

build-all: build-precog build-report

clean-cross:
	cross clean --target $(TARGET)

# --- REPORT (switcher Pi) ---

deploy-report: build-report
	rsync -avz --progress $(RELEASE_DIR)/report \
	    $(REPORT_USER)@$(REPORT_HOST):/usr/local/bin/report.new
	ssh $(REPORT_USER)@$(REPORT_HOST) \
	    'sudo mv /usr/local/bin/report.new /usr/local/bin/report \
	     && sudo systemctl restart report.service'

install-report:
	rsync -av ./report/install.sh $(REPORT_USER)@$(REPORT_HOST):/tmp/report-install.sh
	rsync -av ./report/report.service ./report/report.conf.example \
	    $(REPORT_USER)@$(REPORT_HOST):/tmp/
	ssh $(REPORT_USER)@$(REPORT_HOST) 'bash /tmp/report-install.sh'
	ssh $(REPORT_USER)@$(REPORT_HOST) '\
	    sudo mkdir -p /etc/precrime && \
	    sudo cp /tmp/report.conf.example /etc/precrime/report.conf && \
	    sudo cp /tmp/report.service /etc/systemd/system/report.service && \
	    sudo systemctl daemon-reload && \
	    sudo systemctl enable report.service'
	@echo "Now edit /etc/precrime/report.conf on $(REPORT_HOST) (connector IDs, keyboard device),"
	@echo "then 'make deploy-report'"

logs-report:
	ssh $(REPORT_USER)@$(REPORT_HOST) 'journalctl -u report.service -f'

restart-report:
	ssh $(REPORT_USER)@$(REPORT_HOST) 'sudo systemctl restart report.service'

# --- PRECOG (CCTV encoder Pi) ---

deploy-precog: build-precog
	rsync -avz --progress $(RELEASE_DIR)/precog \
	    $(PRECOG_USER)@$(PRECOG_HOST):/usr/local/bin/precog.new
	ssh $(PRECOG_USER)@$(PRECOG_HOST) \
	    'sudo mv /usr/local/bin/precog.new /usr/local/bin/precog \
	     && sudo systemctl restart precog.service'

install-precog:
	rsync -av ./precog/install.sh $(PRECOG_USER)@$(PRECOG_HOST):/tmp/precog-install.sh
	rsync -av ./precog/precog.service ./precog/precog.conf.example \
	    $(PRECOG_USER)@$(PRECOG_HOST):/tmp/
	ssh $(PRECOG_USER)@$(PRECOG_HOST) 'bash /tmp/precog-install.sh'
	ssh $(PRECOG_USER)@$(PRECOG_HOST) '\
	    sudo mkdir -p /etc/precog && \
	    sudo cp /tmp/precog.conf.example /etc/precog/precog.conf && \
	    sudo cp /tmp/precog.service /etc/systemd/system/precog.service && \
	    sudo systemctl daemon-reload && \
	    sudo systemctl enable precog.service'
	@echo "Now edit /etc/precog/precog.conf on $(PRECOG_HOST) and run 'make deploy-precog'"

logs-precog:
	ssh $(PRECOG_USER)@$(PRECOG_HOST) 'journalctl -u precog.service -f'

restart-precog:
	ssh $(PRECOG_USER)@$(PRECOG_HOST) 'sudo systemctl restart precog.service'
