<!-- INVALID: define + impl (mutually exclusive) -->
<!-- Expected: ValidationError::ConflictingAttributes -->
<skill define="implementation" impl="foo" name="bad" implements="bar">
  This should fail.
</skill>
