<!-- INVALID: name + interface on invocation (mutually exclusive) -->
<!-- Expected: ValidationError::ConflictingAttributes -->
<skill name="foo" interface="bar">
  This should fail.
</skill>
