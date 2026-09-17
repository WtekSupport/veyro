"""Remove Co-authored-by trailers from commit messages (stdin -> stdout)."""
import re
import sys

TRAILER = re.compile(r"(?m)^Co-[Aa]uthored-[Bb]y:.*$\n?")

msg = sys.stdin.read()
msg = TRAILER.sub("", msg)
msg = msg.rstrip() + "\n"
sys.stdout.write(msg)
