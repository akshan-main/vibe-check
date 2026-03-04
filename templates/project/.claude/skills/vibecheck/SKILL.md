---
name: vibecheck
description: On-demand comprehension quiz about your current code changes
disable-model-invocation: true
---

Run a VibeCheck quiz right now based on the current git diff.

STEP 1: Run these git commands to gather context:
- `git diff --name-only` and `git diff --staged --name-only` to get changed files
- `git diff --unified=3` and `git diff --staged --unified=3` to get the diff (truncate to 2000 chars if very long)

If there are no changes, say "No code changes to quiz on. Make some changes first!" and stop.

STEP 2: Create ONE multiple-choice question based on the diff.

QUESTION RULES (critical — this determines if the tool is useful or annoying):
- Ask about BEHAVIOR and CONSEQUENCES, never syntax or trivia
- The user should walk away understanding what their code change DOES in the real world
- Frame questions from the perspective of a user/system interacting with the changed code

GOOD question patterns (use these):
  * "After this change, what happens when [specific user action or edge case]?"
  * "What problem does this change fix, and what was happening before?"
  * "If [realistic scenario], what would this code do differently now?"
  * "What could break if [this related component/input] behaves unexpectedly?"
  * "A user reports [symptom]. Based on this change, what's the most likely cause?"

BAD question patterns (never use these):
  * "What does [language keyword/syntax] mean?" — this is a textbook, not a quiz
  * "What is the return type of [function]?" — irrelevant to understanding
  * "Which design pattern is used here?" — academic, not practical
  * "What library is being imported?" — trivially visible in the diff

WRONG ANSWERS must be plausible. Each wrong option should be something a developer who DIDN'T read the diff carefully might believe. Never use obviously absurd options.

Have exactly 4 options (use labels "A", "B", "C", "D"). One clearly correct.

Ask it using AskUserQuestion with:
- header: "VibeCheck"
- 4 options labeled A, B, C, D
- multiSelect: false

STEP 3: After the user answers, respond with:
1. The correct answer
2. A clear explanation of WHY — connect it to the actual code change (reference specific lines/functions from the diff)
3. If they got it wrong: why their choice was wrong and what part of the diff contradicts it
4. One practical takeaway: a rule of thumb they can apply when reviewing similar code in the future

End your message with: [vibecheck:done]

IMPORTANT: Do NOT use Edit, Write, or any code-modifying tools. This is learning-only.
