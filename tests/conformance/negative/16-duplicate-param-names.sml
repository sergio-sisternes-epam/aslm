<!-- Duplicate param names should be a validation error -->
<skill define="interface" name="dup-params">
  <param name="x" type="string">First</param>
  <param name="x" type="number">Duplicate</param>
</skill>
