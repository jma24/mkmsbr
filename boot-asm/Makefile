# Assemble the three boot-record blobs to 512-byte raw binaries.
# Output: build/{mbr,fat32_pbr,ntfs_pbr}.bin
#
# Build is also invoked from mkmsbr's top-level build.rs directly via `nasm`
# when the `embed-boot-asm` cargo feature is on. This Makefile exists for
# manual iteration during NASM development.

NASM ?= nasm
NASM_FLAGS := -f bin

SRC := mbr_xp.asm mbr_win7.asm fat32_pbr_bootmgr.asm fat32_pbr_ntldr.asm
OUT := $(SRC:%.asm=build/%.bin)

# Multi-sector variants: assembled as sector0.bin + sector1.bin under
# build/<variant>/ (consumers concatenate them — see mkmsbr/build.rs).
MULTI_VARIANTS := fat32_pbr_bootmgr ntfs_pbr_bootmgr
MULTI_SECTORS := sector0.bin sector1.bin
MULTI_OUT := $(foreach v,$(MULTI_VARIANTS),$(foreach s,$(MULTI_SECTORS),build/$(v)/$(s)))

# Test fixtures (not shipped, only used by tests/qemu_*.rs).
TEST_SRC := fake_bootmgr.asm fake_pbr.asm
TEST_OUT := $(TEST_SRC:%.asm=build/%.bin)

all: $(OUT) $(MULTI_OUT)

build/%/sector0.bin: %/sector0.asm
	@mkdir -p $(dir $@)
	$(NASM) $(NASM_FLAGS) -o $@ $<
build/%/sector1.bin: %/sector1.asm
	@mkdir -p $(dir $@)
	$(NASM) $(NASM_FLAGS) -o $@ $<

# Boot blobs must be exactly 512 bytes.
$(OUT): SIZE_CHECK := 512

test-fixtures: $(TEST_OUT)

build:
	mkdir -p build

build/%.bin: %.asm | build
	$(NASM) $(NASM_FLAGS) -o $@ $<
	@if [ -n "$(SIZE_CHECK)" ]; then \
	  size=$$(stat -f%z $@ 2>/dev/null || stat -c%s $@); \
	  if [ "$$size" != "$(SIZE_CHECK)" ]; then \
	    echo "ERROR: $@ is $$size bytes; must be exactly $(SIZE_CHECK)"; \
	    exit 1; \
	  fi; \
	fi

clean:
	rm -rf build

.PHONY: all clean test-fixtures
