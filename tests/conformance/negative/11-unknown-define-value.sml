<!-- INVALID: unknown define value -->
<!-- Expected: ValidationError::UnknownDefineValue -->
<skill define="something-else" name="bad">
  define must be "interface" or "implementation".
</skill>
