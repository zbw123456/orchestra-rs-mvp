#!/usr/bin/env python3
"""Example non-LLM provider: consume the task envelope and analyze its prompt."""
from collections import Counter
import json
import re
import sys

envelope = json.load(sys.stdin)
if envelope.get("schema_version") != 1:
    raise SystemExit("unsupported task protocol")
words = re.findall(r"\w+", envelope["task"]["prompt"].lower())
print(json.dumps({"agent": "text", "word_count": len(words), "top_words": Counter(words).most_common(5)}))
