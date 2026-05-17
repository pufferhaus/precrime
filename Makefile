# PRECRIME build + deploy targets.
#
# Builds run on macOS via a native linux/arm64 Docker container (no QEMU).
# One-time setup: make build-image
# Override hosts per invocation:
#   make deploy-report REPORT_HOST=192.168.50.10
#   make logs-precog   PRECOG_HOST=precog-02-cctv-door.local

TARGET      := aarch64-unknown-linux-gnu
RELEASE_DIR := target/$(TARGET)/release

# docker run template: mount cargo registry cache + workspace, build in native arm64 container.
# Mount only ~/.cargo/registry (downloaded crate sources) not ~/.cargo/bin (macOS binaries).
DOCKER_BUILD := docker run --rm --platform linux/arm64 \
    -v $(HOME)/.cargo/registry:/root/.cargo/registry \
    -v $(CURDIR):/project \
    -w /project \
    precrime-cross:aarch64 \
    cargo build --release --target $(TARGET)

PRECOG_HOST ?= precog-01.local
PRECOG_USER ?= precog
REPORT_HOST ?= report.local
REPORT_USER ?= pi

.PHONY: help check fmt clippy test \
        build-image build-precog build-report build-all clean-cross \
        deploy-report install-report logs-report restart-report \
        deploy-precog install-precog logs-precog restart-precog

help:
	@echo "One-time setup (after cloning or changing Cross.Dockerfile):"
	@echo "  make build-image"
	@echo ""
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

# --- Docker image ---

build-image:
	docker build --platform linux/arm64 -f Cross.Dockerfile -t precrime-cross:aarch64 .

# --- Local workspace ---

check:
	cargo check

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings

test:
	cargo test

# ---- Build for aarch64 (Raspberry Pi 5) via native arm64 container ----

build-precog:
	$(DOCKER_BUILD) --package precog

build-report:
	$(DOCKER_BUILD) --package report

build-all: build-precog build-report

clean-cross:
	rm -rf target/$(TARGET)

# --- REPORT (switcher Pi) ---

deploy-report: build-report
	rsync -avz --progress $(RELEASE_DIR)/report \
	    $(REPORT_USER)@$(REPORT_HOST):/tmp/report.new
	ssh $(REPORT_USER)@$(REPORT_HOST) \
	    'sudo mv /tmp/report.new /usr/local/bin/report \
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
	    $(PRECOG_USER)@$(PRECOG_HOST):/tmp/precog.new
	ssh $(PRECOG_USER)@$(PRECOG_HOST) \
	    'sudo mv /tmp/precog.new /usr/local/bin/precog \
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
