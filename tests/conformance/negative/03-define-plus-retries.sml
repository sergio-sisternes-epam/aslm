<!-- INVALID: define + retries (definitions can't execute) -->
<!-- Expected: ValidationError::ConflictingAttributes -->
<skill define="interface" name="bad" retries="3">
  This should fail.
</skill>
