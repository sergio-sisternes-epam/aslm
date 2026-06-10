<!-- Error: contract extends graphs must be acyclic -->
<skill define="contract" name="contract-a" extends="contract-b">
  <field name="id" type="string" />
</skill>

<skill define="contract" name="contract-b" extends="contract-a">
  <field name="id" type="string" />
</skill>
