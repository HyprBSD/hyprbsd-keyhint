# Security

hyprbsd-keyhint runs as you. It starts the programs you pick in HyprBSD's launcher, reads
Hyprland's sockets and shows a card. Please report anything that makes it start something other
than the entry you picked, run a command it read from Hyprland, or show a card that takes your
keyboard or mouse.

## Reporting a problem

Don't open a public issue. Report it privately, either:

- through GitHub: the repository's **Security** tab, **Report a vulnerability**; or
- by email to security@hyprbsd.org.

Please include the FreeBSD version (`freebsd-version -ku`), hyprbsd-keyhint's version (on
HyprBSD, `pkg info hyprbsd-keyhint`; built from source, the tag or commit), and the steps that
show the problem.

## Supported versions

Fixes go into the latest release.
