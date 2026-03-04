---
name: quiz
description: On-demand product comprehension quiz about your current code changes
---

Run a VibeCheck quiz right now based on the current git diff.

STEP 1: Run these git commands to gather context:
- `git diff --name-only` and `git diff --staged --name-only` to get changed files
- `git diff --unified=3` and `git diff --staged --unified=3` to get the diff (truncate to 2000 chars if very long)

If there are no changes, say "No code changes to quiz on. Make some changes first!" and stop.

STEP 2: Analyze the diff and create ONE multiple-choice question.

First, figure out what ACTUALLY CHANGED in the product by reading the diff. Don't count lines — understand the intent:
- Was a feature ADDED? (new capability that didn't exist before)
- Was a feature CHANGED? (existing behavior now works differently)
- Was something REMOVED? (capability or safeguard that's now gone)
- Was it a FIX? (broken thing that now works)

Then ask a question that tests whether the developer understands the REAL-WORLD IMPACT of this change on their product. Vibe coders build products — they need to understand what their product does, not how to read code.

QUESTION FORMULA — pick one:
  * WHAT CHANGED: "Before this change, [X happened]. What happens now instead?"
  * WHAT'S NEW: "A user tries [action] for the first time. What do they experience?"
  * WHAT'S GONE: "You removed [feature/check/step]. What can users do now that they couldn't before — or what protection is no longer there?"
  * SIDE EFFECTS: "This change also affects [related area]. What's different there now?"
  * EDGE CASE: "A user does [unusual but realistic action]. How does your app handle it after this change?"
  * LIMITS: "What's the maximum/minimum [value/count/size] this feature now supports? What happens at the boundary?"
  * DATA: "After this change, what new data is being stored/sent/exposed? Who can see it?"

NEVER ASK:
  * About code syntax, language features, or programming concepts
  * About which library or framework is used
  * Anything a developer would need to read code to answer — the question should be answerable by someone who understands the PRODUCT but not the code
  * Generic questions unrelated to this specific diff

WRONG ANSWERS: Each should be a plausible misunderstanding of what the product change does. Things a developer might assume if they didn't pay attention to what Claude actually built.

Format: exactly 4 options (labels "A", "B", "C", "D"), one correct. Ask via AskUserQuestion with header: "VibeCheck", multiSelect: false.

STEP 3: After the user answers:
1. Explain the correct answer in plain language — what the product does now and why
2. If wrong: explain what they misunderstood about the change and what their answer would have meant for users
3. A PROMPTING TIP: suggest how they could have prompted Claude differently to get a better result or avoid the gap this question exposed. For example: "Next time, try: 'Add rate limiting AND return a friendly error message with a Retry-After header when the limit is hit.'" This helps them write more complete prompts in the future.

End your message with: [vibecheck:done]

IMPORTANT: Do NOT use Edit, Write, or any code-modifying tools. This is learning-only.
