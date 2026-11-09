#!/usr/bin/env bash
# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
# ==============================================================================
# verify-git-privacy.sh
# Pre-push privacy & identity audit for lighthouselogic/ok-fpga-server.
# Ensures zero personal names, personal email addresses, or local machine paths
# exist in git config, commit history, tags, or tracked files.
# ==============================================================================
set -euo pipefail

ALLOWED_EMAIL_DOMAIN="${ALLOWED_EMAIL_DOMAIN:-lighthouselogic.dev}"
EXPECTED_USER_NAME="${EXPECTED_USER_NAME:-Lighthouse Logic}"
EXPECTED_USER_EMAIL="${EXPECTED_USER_EMAIL:-admin@lighthouselogic.dev}"
# Optional regex pattern of additional forbidden tokens
EXTRA_FORBIDDEN_REGEX="${EXTRA_FORBIDDEN_REGEX:-}"

FAILURES=0

echo "== [1/5] Checking repository-local git config =="
LOCAL_NAME="$(git config --local user.name || true)"
LOCAL_EMAIL="$(git config --local user.email || true)"

if [[ "$LOCAL_NAME" != "$EXPECTED_USER_NAME" ]]; then
  echo "  [FAIL] Local git user.name is '$LOCAL_NAME' (expected '$EXPECTED_USER_NAME')"
  FAILURES=$((FAILURES + 1))
else
  echo "  [OK]   Local git user.name = $LOCAL_NAME"
fi

if [[ "$LOCAL_EMAIL" != "$EXPECTED_USER_EMAIL" ]]; then
  echo "  [FAIL] Local git user.email is '$LOCAL_EMAIL' (expected '$EXPECTED_USER_EMAIL')"
  FAILURES=$((FAILURES + 1))
else
  echo "  [OK]   Local git user.email = $LOCAL_EMAIL"
fi

echo "== [2/5] Auditing all commit Author and Committer identities (--all) =="
if git rev-parse --verify HEAD >/dev/null 2>&1; then
  BAD_COMMITS="$(git log --all --format='%H | Author: %an <%ae> | Committer: %cn <%ce>' | grep -vE "<[^>@]+@${ALLOWED_EMAIL_DOMAIN}>.*<[^>@]+@${ALLOWED_EMAIL_DOMAIN}>" || true)"
  if [[ -n "$BAD_COMMITS" ]]; then
    echo "  [FAIL] Found commits with non-${ALLOWED_EMAIL_DOMAIN} author/committer emails:"
    echo "$BAD_COMMITS" | sed 's/^/    /'
    FAILURES=$((FAILURES + 1))
  else
    echo "  [OK]   All commits use @${ALLOWED_EMAIL_DOMAIN} author and committer emails."
  fi
else
  echo "  [WARN] Repository has no commits yet."
fi

echo "== [3/5] Auditing annotated Git tags =="
BAD_TAGS="$(git for-each-ref --format='%(refname:short) | %(taggername) %(taggeremail)' refs/tags | grep -v '|  $' | grep -vE "@${ALLOWED_EMAIL_DOMAIN}>" || true)"
if [[ -n "$BAD_TAGS" ]]; then
  echo "  [FAIL] Found annotated tags with non-${ALLOWED_EMAIL_DOMAIN} tagger email:"
  echo "$BAD_TAGS" | sed 's/^/    /'
  FAILURES=$((FAILURES + 1))
else
  echo "  [OK]   No annotated tags leak non-${ALLOWED_EMAIL_DOMAIN} identities."
fi

echo "== [4/5] Scanning tracked files for personal email domains & home directory paths =="
DEFAULT_LEAK_REGEX='(@gmail\.com|@yahoo\.com|@hotmail\.com|@outlook\.com|@icloud\.com|C:\\Users\\|C:/Users/|/home/[a-zA-Z0-9_-]+/|/Users/[a-zA-Z0-9_-]+/)'
LEAK_MATCHES="$(git grep -I -n -E "$DEFAULT_LEAK_REGEX" -- . ':!scripts/verify-git-privacy.sh' ':!scripts/scrub-git-history.sh' ':!AGENT-HANDOFF*.md' || true)"
if [[ -n "$LEAK_MATCHES" ]]; then
  echo "  [FAIL] Found potential personal email or local path leaks in tracked files:"
  echo "$LEAK_MATCHES" | sed 's/^/    /'
  FAILURES=$((FAILURES + 1))
else
  echo "  [OK]   No personal email domains or local home directory paths found in tracked files."
fi

echo "== [5/5] Checking extra forbidden patterns (if provided) =="
if [[ -n "$EXTRA_FORBIDDEN_REGEX" ]]; then
  EXTRA_FILE_MATCHES="$(git grep -I -i -n -E "$EXTRA_FORBIDDEN_REGEX" -- . || true)"
  if [[ -n "$EXTRA_FILE_MATCHES" ]]; then
    echo "  [FAIL] Found forbidden pattern ($EXTRA_FORBIDDEN_REGEX) in tracked files:"
    echo "$EXTRA_FILE_MATCHES" | sed 's/^/    /'
    FAILURES=$((FAILURES + 1))
  fi
  if git rev-parse --verify HEAD >/dev/null 2>&1; then
    EXTRA_LOG_MATCHES="$(git log --all --grep="$EXTRA_FORBIDDEN_REGEX" -i --oneline || true)"
    if [[ -n "$EXTRA_LOG_MATCHES" ]]; then
      echo "  [FAIL] Found forbidden pattern ($EXTRA_FORBIDDEN_REGEX) in commit messages:"
      echo "$EXTRA_LOG_MATCHES" | sed 's/^/    /'
      FAILURES=$((FAILURES + 1))
    fi
  fi
  if [[ "$FAILURES" -eq 0 ]]; then
    echo "  [OK]   No matches for EXTRA_FORBIDDEN_REGEX."
  fi
else
  echo "  [INFO] Tip: Run with EXTRA_FORBIDDEN_REGEX='(your_handle|your_lastname)' for stricter checks."
fi

if [[ "$FAILURES" -gt 0 ]]; then
  echo ""
  echo "== RESULT: FAILED ($FAILURES privacy check(s) failed — DO NOT PUSH) =="
  exit 1
fi

echo ""
echo "== RESULT: PASSED (Repository is clean and ready for public release) =="
