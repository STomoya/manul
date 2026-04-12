VENV := .venv
UV := uv

# Dynamically find the directory containing libpython.so
# sysconfig.get_config_var('LIBDIR') points to the library folder
PYTHON_LIB_DIR := $(shell $(UV) run python -c "import sysconfig; print(sysconfig.get_config_var('LIBDIR'))")

# Export it for all subsequent commands
# This ensures it's available to 'uv run' and other binaries
export LD_LIBRARY_PATH := $(PYTHON_LIB_DIR)$(if $(LD_LIBRARY_PATH),:$(LD_LIBRARY_PATH))

.PHONY: info rstest

info:
	@echo LD_LIBRARY_PATH: $(LD_LIBRARY_PATH)

# Use this to debug and make sure the .so actually exists there
rstest:
	cargo test

# test with coverage. Excludes lib.rs files because they only include module exports.
rscoverage:
	cargo llvm-cov --show-missing-lines --ignore-filename-regex="lib.rs"
