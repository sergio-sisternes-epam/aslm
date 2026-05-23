<!-- INVALID: define + interface (mutually exclusive) -->
<!-- Expected: ValidationError::ConflictingAttributes -->
<skill define="interface" interface="something" name="bad">
  This should fail.
</skill>
