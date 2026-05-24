<!-- Interface definition with self-closing param and returns declarations -->
<skill define="interface" name="data-transform">
  <param name="input" type="path" required="true" />
  <param name="verbose" type="boolean" default="false" />
  <param name="limit" type="number" default="100" />
  <returns name="output" type="path" />
  <returns name="count" type="number" />
</skill>
