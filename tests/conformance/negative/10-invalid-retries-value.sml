<!-- INVALID: retries is not an integer -->
<!-- Expected: ValidationError::InvalidAttributeValue -->
<skill interface="something" retries="abc">
  Bad retries value.
</skill>
