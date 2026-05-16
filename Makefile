# PRECRIME build + deploy targets.
#
# Build happens ON THE PI via SSH. No local cross-compile setup.
# Override hosts per invocation:
#   make deploy-report REPORT_HOST=192.168.50.10
#   make logs-precog   PRECOG_HOST=precog-02-cctv-door.local

REPORT_HOST    ?= report.local
PRECOG_HOST    ?= precog-02-cctv-door.local
SSH_USER       ?= cody
REPO_PATH      ?= /home/$(SSH_USER)/precrime

.PHONY: help check fmt clippy test \
        sync-report build-report deploy-report install-report logs-report restart-report \
        sync-precog build-precog deploy-precog install-precog logs-precog restart-precog

help:
	@echo "Local (Mac) workspace targets:"
	@echo "  check            cargo check whole workspace"
	@echo "  fmt              cargo fmt"
	@echo "  clippy           cargo clippy -D warnings"
	@echo "  test             cargo test (pure-Rust modules only on macOS)"
	@echo ""
	@echo "REPORT  (override REPORT_HOST=...):"
	@echo "  sync-report      rsync source to \$$(REPORT_HOST)"
	@echo "  build-report     sync + cargo build --release -p report"
	@echo "  deploy-report    build + install binary + restart service"
	@echo "  install-report   first-time install (deps + systemd unit + config template)"
	@echo "  logs-report      tail journalctl -u report.service -f"
	@echo "  restart-report   systemctl restart report.service"
	@echo ""
	@echo "PRECOG  (override PRECOG_HOST=...):"
	@echo "  sync-precog      rsync source to \$$(PRECOG_HOST)"
	@echo "  build-precog     sync + cargo build --release -p precog"
	@echo "  deploy-precog    build + install binary + restart service"
	@echo "  install-precog   first-time install"
	@echo "  logs-precog      tail journalctl -u precog.service -f"
	@echo "  restart-precog   systemctl restart precog.service"

# --- Local workspace ---

check:
	cargo check

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings

test:
	cargo test

# --- REPORT (switcher Pi) ---

sync-report:
	rsync -av --delete --exclude target/ --exclude .git/ --exclude .claude/ \
	    ./ $(SSH_USER)@$(REPORT_HOST):$(REPO_PATH)/

build-report: sync-report
	ssh $(SSH_USER)@$(REPORT_HOST) 'cd $(REPO_PATH) && cargo build --release -p report'

deploy-report: build-report
	ssh $(SSH_USER)@$(REPORT_HOST) '\
	    sudo cp $(REPO_PATH)/target/release/report /usr/local/bin/report && \
	    sudo systemctl restart report.service'

install-report:
	rsync -av ./report/install.sh $(SSH_USER)@$(REPORT_HOST):/tmp/report-install.sh
	rsync -av ./report/report.service ./report/report.conf.example \
	    $(SSH_USER)@$(REPORT_HOST):/tmp/
	ssh $(SSH_USER)@$(REPORT_HOST) 'bash /tmp/report-install.sh'
	ssh $(SSH_USER)@$(REPORT_HOST) '\
	    sudo mkdir -p /etc/precrime && \
	    sudo cp /tmp/report.conf.example /etc/precrime/report.conf && \
	    sudo cp /tmp/report.service /etc/systemd/system/report.service && \
	    sudo systemctl daemon-reload && \
	    sudo systemctl enable report.service'
	@echo "Now edit /etc/precrime/report.conf on $(REPORT_HOST) (connector IDs, keyboard device),"
	@echo "install libndi per docs/plans/2026-05-16-report-switcher.md Task 2, then 'make deploy-report'"

logs-report:
	ssh $(SSH_USER)@$(REPORT_HOST) 'sudo journalctl -u report.service -f'

restart-report:
	ssh $(SSH_USER)@$(REPORT_HOST) 'sudo systemctl restart report.service'

# --- PRECOG (CCTV encoder Pi) ---

sync-precog:
	rsync -av --delete --exclude target/ --exclude .git/ --exclude .claude/ \
	    ./ $(SSH_USER)@$(PRECOG_HOST):$(REPO_PATH)/

build-precog: sync-precog
	ssh $(SSH_USER)@$(PRECOG_HOST) 'cd $(REPO_PATH) && cargo build --release -p precog'

deploy-precog: build-precog
	ssh $(SSH_USER)@$(PRECOG_HOST) '\
	    sudo cp $(REPO_PATH)/target/release/precog /usr/local/bin/precog && \
	    sudo systemctl restart precog.service'

install-precog:
	rsync -av ./precog/install.sh $(SSH_USER)@$(PRECOG_HOST):/tmp/precog-install.sh
	rsync -av ./precog/precog.service ./precog/precog.conf.example \
	    $(SSH_USER)@$(PRECOG_HOST):/tmp/
	ssh $(SSH_USER)@$(PRECOG_HOST) 'bash /tmp/precog-install.sh'
	ssh $(SSH_USER)@$(PRECOG_HOST) '\
	    sudo mkdir -p /etc/precog && \
	    sudo cp /tmp/precog.conf.example /etc/precog/precog.conf && \
	    sudo cp /tmp/precog.service /etc/systemd/system/precog.service && \
	    sudo systemctl daemon-reload && \
	    sudo systemctl enable precog.service'
	@echo "Now edit /etc/precog/precog.conf on $(PRECOG_HOST) (NDI name, format, framerate)"
	@echo "and run 'make deploy-precog' to start streaming"

logs-precog:
	ssh $(SSH_USER)@$(PRECOG_HOST) 'sudo journalctl -u precog.service -f'

restart-precog:
	ssh $(SSH_USER)@$(PRECOG_HOST) 'sudo systemctl restart precog.service'
