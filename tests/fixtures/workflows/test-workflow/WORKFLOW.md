---
name: test-workflow
description: A test workflow that chains echo commands
timeout: 30
---
# Echo chain: first step outputs "hello", second step receives it via ${prev}
echo say hello
echo say ${prev}
