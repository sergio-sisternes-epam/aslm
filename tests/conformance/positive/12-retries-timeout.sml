<!-- Retries and timeout attributes -->
<skill interface="flaky-api" retries="3" timeout="30s" on-failure="skip">
  Fetch the latest deployment status.
</skill>
