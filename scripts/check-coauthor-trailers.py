"""Exit 0 if commit message has no Co-authored-by trailers, else 1."""
import re
import sys
from pathlib import Path

TRAILER = re.compile(r"(?m)^Co-[Aa]uthored-[Bb]y:", re.IGNORECASE)

path = Path(sys.argv[1]) if len(sys.argv) > 1 else None
text = path.read_text(encoding="utf-8") if path else sys.stdin.read()
sys.exit(1 if TRAILER.search(text) else 0)
