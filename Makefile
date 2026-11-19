# hyprbsd-keyhint's build and checks, for FreeBSD's make.
#
#   make            the program (cargo)
#   make install    it under DESTDIR and PREFIX
#   make dist       hyprbsd-keyhint-<version>.tar.gz in CARGO_TARGET_DIR,
#                   from the tag v<version>
#   make check      rustfmt's check, the tests, clippy, then
#                   tests/session-test.sh on a release build (needs sway,
#                   grim, jq, python3.12 with PyGObject and dbus-run-session)
#   make deny       cargo-deny: the crates' licences, RustSec's advisories,
#                   duplicate versions and sources (deny.toml)
#   make clean      cargo's build output
#
# SPDX-License-Identifier: BSD-2-Clause

CARGO?=		cargo
PREFIX?=	/usr/local
CARGO_TARGET_DIR?=	${.CURDIR}/target
MANIFEST=	${.CURDIR}/Cargo.toml
CARGO_ARGS=	--manifest-path ${MANIFEST} --target-dir ${CARGO_TARGET_DIR}
VERSION!=	sed -n '/^\[package\]/,/^\[/s/^version = "\(.*\)"/\1/p' ${MANIFEST}

all:
	${CARGO} build --release --locked ${CARGO_ARGS}

install:
	install -d ${DESTDIR}${PREFIX}/bin
	install -m 0755 ${CARGO_TARGET_DIR}/release/hyprbsd-keyhint ${DESTDIR}${PREFIX}/bin/

dist:
	mkdir -p ${CARGO_TARGET_DIR}
	git -C ${.CURDIR} archive --format=tar.gz --prefix=hyprbsd-keyhint-${VERSION}/ \
		-o ${CARGO_TARGET_DIR}/hyprbsd-keyhint-${VERSION}.tar.gz v${VERSION}

check:
	${CARGO} fmt --check --manifest-path ${MANIFEST}
	${CARGO} test --release --locked ${CARGO_ARGS}
	${CARGO} clippy --all-targets --locked ${CARGO_ARGS} -- -D warnings
	${CARGO} build --release --locked ${CARGO_ARGS}
	${.CURDIR}/tests/session-test.sh ${CARGO_TARGET_DIR}/release/hyprbsd-keyhint

deny:
	${CARGO} deny --manifest-path ${MANIFEST} check

clean:
	${CARGO} clean ${CARGO_ARGS}

.PHONY: all install dist check deny clean
