Supply a query filter and result formatter in one. If the filter doesn't match, return an empty array or nothing. If the filter does match, return an array of individual results.

This function always returns greeting messages whatever the input:

```javascript
function filter(args) {
  return ["Hello world"];
}
```



# FAQ

**Why does my function have to be in a function? Can't I just write an expression directly?**

This would work for very simple examples but wouldn't be useful for most tasks. For example, it wouldn't allow performing multiple steps, `if` statements, etc.
