# Claude Notes

## Git Workflow for Merging PRs

Always use fast-forward merges to ensure commits are signed and GitHub auto-closes PRs:

1. **Rebase branch to main/master**
   ```bash
   git fetch origin master
   git rebase origin/master
   ```

2. **Push branch to origin** (so GitHub sees it's up to date)
   ```bash
   git push origin <branch-name> --force-with-lease
   ```

3. **Fast-forward merge to main/master**
   ```bash
   git checkout master
   git merge <branch-name> --ff-only
   ```

4. **Push main/master to origin** (GitHub will auto-close the PR)
   ```bash
   git push origin master
   ```

5. **Clean up branches**
   ```bash
   git branch -d <branch-name>
   git push origin --delete <branch-name>
   ```

**Why fast-forward only:**
- Keeps commit SHAs intact so GitHub recognizes the merge and auto-closes PRs
- Preserves signed commits (no new merge commit created)

**Do NOT use `gh pr merge`** - it bypasses local signing keys and commits won't be verified.

## Testing

Always wait for user to test changes before pushing commits.
