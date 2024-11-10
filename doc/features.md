# Features

This list of features, implemented and TODO, is for keeping track of development during prototyping.

## Function execution

- Load functions from local disk.
- Catch and report exception executing functions.
- Catch and report exception loading JS file.
- Supply global context to all invocations.
- Supply input to all function invocations.
- Store result from function execution.
- Expose and store console.log, console.error
- TODO
  - Heap limit, OOM kill.
  - Execution time limit and kill.
  - Limit on file size.
  - Validate function on load for size.
- Description and author fields as optional.

## Events

- TODO
  - Work metadata available
  -

## Lifecycle & Queueing
- TODO
   - Upload via API
   - Validate on upload - parsing, size
   - Expire functions if not used
   - Allow deletion of functions via handle.
   - Expire results

## Presentation
- TODO
  - Formatting function for page of results.
  - List of functions.
  - Output of functions.

## Sources
- TODO
  - Crossref
  - DataCite
  - ROR
  - Hacker News
  - Hypothesis
  - Wikipedia

## Sample scripts

- TODO
  - New DOIs
  - Cited DOIs
  - Invalid ORCID IDs
  - Invalid RORs
  - Invalid ISBNs
  - Works that cite papers in journal
  - Works that cite works with ROR
  - Works with 'fish' and 'chips' in the abstract
