Your function serves as both a filter and a transformer. If you want to match and transform the input, return an array of responses. If you don't, return an empty list or null. If you don't return anything, the function run will produce an error message.

Your snippet of JavaScript must have a function named `f`, which takes the event as an argument. You may put other code in the JavaScript if you like, as long as the `f` function is available.

This function always returns greeting messages whatever the input:

```javascript
function f(args) {
  return ["Hello world"];
}
```

You can use:
 - plain JavaScript features

You can't use:
 - setTimeout
 - network



# License

All the input data to your function is CC0. The output of your function, and the function itself, is also considered CC0. Other users may be able to discover your function and its output.

Don't put any secrets in your function. You can put ORCIDs and email addresses in the function if you don't mind them being public.

If you want to discuss a private code / data feature, please get in touch.

# FAQ



**Why does my function have to be in a function? Can't I just write an expression directly?**

This would work for very simple examples but wouldn't be useful for most tasks. For example, it wouldn't allow performing multiple steps, `if` statements, etc.

**When does my JavaScript run?**

The `f` function will be run once for each event. Your file may be loaded, executed and cached. If you run other code outside the function it may run outside the execution of an event. If you're curious, you might try writing this code:

```javascript
let c = 0;
function f(args) {
  c += 1;
  return [c];
}
```

On a busy day, the file might be cached between events, and if it receives 3 events in quick succession the value of `c` may be incremented, returning the values 1,2,3. On a slow day, the file might not be cached, and the value of `c` might always be zero.

Don't rely on this behaviour, it may change.
