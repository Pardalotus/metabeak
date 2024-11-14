# Features

This list of features, implemented and TODO, is for keeping track of development during prototyping.

## Function execution

- Load functions from local disk to database.
- Catch and report exception executing functions.
- Catch and report exception loading JS file.
- Supply global context to all invocations.
- Supply Event input to all function invocations.
- Store result from function execution.
- TODO
  - Expose and store console.log, console.error
  - Heap limit, OOM kill.
  - Execution time limit and kill.
  - Limit on file size.
  - Validate function on load for size.
  - Disable broken functions and report
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
   - Expire inputs

## Presentation
- TODO
  - Formatting function for page of results.
  - List of functions.
  - Output of functions.
  - Show output as HTML
  - Show output as Activity Stream
  - Investigate run, show inputs.


## Sources
- TODO
  - Crossref
  - DataCite
  - ROR
  - Hacker News
  - Hypothesis
  - Wikipedia
  - Rogue Scholar


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


## Design questions

what happens when you re-upload a suspended or deleted handler function?
