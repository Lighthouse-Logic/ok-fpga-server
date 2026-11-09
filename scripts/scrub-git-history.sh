#!/usr/bin/env bash
# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
# ==============================================================================
# scrub-git-history.sh
# Configures repository-local Git identity ("Lighthouse Logic <admin@lighthouselogic.dev>"),
# installs a pre-commit identity guard hook, and either:
#   --rewrite (default): rewrites all existing commits/tags to the role identity
#   --squash           : creates a single clean initial commit on an orphan branch
# ==============================================================================
set -euo pipefail

MODE="${1:---rewrite}"
NEW_NAME="${NEW_NAME:-Lighthouse Logic}"
NEW_EMAIL="${NEW_EMAIL:-admin@lighthouselogic.dev}"
INITIAL_COMMIT_MSG="${INITIAL_COMMIT_MSG:-feat: initial public release of ok-fpga-server}"

echo "== [1/4] Setting repository-local Git author/committer identity =="
git config --local user.name "$NEW_NAME"
git config --local user.email "$NEW_EMAIL"
echo "  Set local user.name  = $(git config --local user.name)"
echo "  Set local user.email = $(git config --local user.email)"

echo "== [2/4] Installing .git/hooks/pre-commit privacy guard =="
mkdir -p .git/hooks
cat << 'EOF' > .git/hooks/pre-commit
#!/usr/bin/env bash
EMAIL="$(git config user.email || true)"
if [[ ! "$EMAIL" =~ @lighthouselogic\.dev$ ]]; then
  echo "[BLOCKED] Commit aborted: git user.email ('$EMAIL') must end with @lighthouselogic.dev"
  echo "Run: git config --local user.name 'Lighthouse Logic' && git config --local user.email 'admin@lighthouselogic.dev'"
  exit 1
fi
EOF
chmod +x .git/hooks/pre-commit
echo "  Installed .git/hooks/pre-commit guard."

echo "== [3/4] Scrubbing commit history ($MODE) =="
if [[ "$MODE" == "--squash" ]]; then
  CURRENT_BRANCH="$(git symbolic-ref --short HEAD 2>/dev/null || echo "main")"
  git checkout --orphan __clean_release_branch
  git add -A
  GIT_AUTHOR_NAME="$NEW_NAME" \
  GIT_AUTHOR_EMAIL="$NEW_EMAIL" \
  GIT_COMMITTER_NAME="$NEW_NAME" \
  GIT_COMMITTER_EMAIL="$NEW_EMAIL" \
    git commit -m "$INITIAL_COMMIT_MSG"
  git branch -D "$CURRENT_BRANCH" 2>/dev/null || true
  git branch -m "$CURRENT_BRANCH"
  echo "  Squashed repository into a single clean commit on branch '$CURRENT_BRANCH'."
elif [[ "$MODE" == "--rewrite" ]]; then
  FILTER_BRANCH_SQUELCH_WARNING=1 git filter-branch -f --env-filter "
    export GIT_AUTHOR_NAME='$NEW_NAME'
    export GIT_AUTHOR_EMAIL='$NEW_EMAIL'
    export GIT_COMMITTER_NAME='$NEW_NAME'
    export GIT_COMMITTER_EMAIL='$NEW_EMAIL'
  " --tag-name-filter cat -- --branches --tags
  # Remove backup refs created by git filter-branch
  git for-each-ref --format="%(refname)" refs/original/ | xargs -n 1 git update-ref -d 2>/dev/null || true
  echo "  Rewrote all branch and tag commits to '$NEW_NAME <$NEW_EMAIL>'."
else
  echo "Usage: $0 [--rewrite | --squash]"
  exit 1
fi

echo "== [4/4] Purging reflogs and unreachable Git objects =="
git reflog expire --expire=now --all
git gc --prune=now --aggressive
echo "  Done. Run bash scripts/verify-git-privacy.sh to confirm."
