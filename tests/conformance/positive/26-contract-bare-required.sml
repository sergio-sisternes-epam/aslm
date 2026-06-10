<!-- Bare required in self-closing and text-content forms, plus extends and contract refs -->
<skill define="contract" name="tracker-metadata" version="1.0">
  <field name="platform" type="enum" values="github|jira|ado" required />
  <field name="ref" type="string" required>Stable tracker reference</field>
</skill>

<skill define="contract" name="planning-input" extends="tracker-metadata" version="1.0">
  <field name="summary" type="string" required />
  <field name="tracker" type="contract:tracker-metadata" required>Structured tracker metadata</field>
</skill>

<skill define="interface" name="planner">
  <param name="spec" type="contract:planning-input" required>Planning input</param>
  <returns name="result" type="contract:tracker-metadata">Resolved tracker payload</returns>
</skill>
