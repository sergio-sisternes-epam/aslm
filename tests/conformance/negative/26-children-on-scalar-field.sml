<!-- Error: scalar field types cannot contain child fields -->
<skill define="contract" name="children-on-scalar-field">
  <field name="title" type="string">
    <field name="nested" type="string" />
  </field>
</skill>
