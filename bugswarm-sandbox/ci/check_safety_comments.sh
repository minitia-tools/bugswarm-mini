#!/usr/bin/env bash
# Check that every `unsafe {` or `unsafe fn` has a preceding `// SAFETY:` comment.
set -euo pipefail

cd "$(dirname "$0")/.."

exit_code=0

find src/ -name '*.rs' -type f | while read -r file; do
    # Find all lines with `unsafe {` or `unsafe fn` (not in comments)
    while IFS=: read -r lineno line; do
        stripped="$(echo "$line" | sed 's|//.*||')"
        if echo "$stripped" | grep -qP '\bunsafe\s*(\{|fn\b)'; then
            # Extract preceding lines and scan backward for SAFETY comment
            block=$(sed -n "1,$((lineno - 1))p" "$file" \
                | awk '{ lines[NR]=$0 }
                  END {
                      for (i = NR; i >= 1; i--) {
                          # Check SAFETY comment FIRST on the original line
                          if (lines[i] ~ /\/\/\s*SAFETY:/) { found=1; break }
                          s = lines[i]
                          gsub(/\/\/.*/, "", s)
                          if (s ~ /^\s*$/ || s ~ /^\s*debug_assert!/ || s ~ /^\s*#\[cfg\(test\)\]/) continue
                          break
                      }
                      print found
                  }')
            if [ "$block" != "1" ]; then
                echo "ERROR: $file:$lineno has unsafe block without preceding // SAFETY: comment"
                echo "  unsafe line: $line"
                exit_code=1
            fi
        fi
    done < <(grep -nP '\bunsafe\s*(\{|fn\b)' "$file")
done

exit $exit_code
