<!-- Full specification contract with nested object and list-of-objects fields -->
<skill define="contract" name="specification-contract" version="1.0">
  <field name="work_item_id" type="string" required>Stable tracker ID</field>
  <field name="tracker" type="object">
    <field name="platform" type="enum" values="github-issues|jira|ado-boards" required />
    <field name="ref" type="string" required />
    <field name="url" type="string" required />
  </field>
  <field name="implementation_id" type="string" required>Unique implementation identifier</field>
  <field name="title" type="string" required>Human-readable summary</field>
  <field name="description" type="string" required>Detailed scope statement</field>
  <field name="acceptance_criteria" type="list" required>Verifiable success conditions</field>
  <field name="complexity" type="enum" values="Patch|Feature|Epic" required />
  <field name="scenario_profile" type="enum" values="Greenfield|Brownfield|Modernisation" required />
  <field name="stack_components" type="list" required>
    <field name="component_id" type="string" required />
    <field name="kind" type="enum" values="app|service|library|infra|docs" required />
    <field name="language" type="string">Null means detect at runtime</field>
    <field name="path_hints" type="list" />
  </field>
  <field name="applicable_standards" type="list">Standards to apply</field>
  <field name="source_traceability" type="list" required>Origin issue or epic links</field>
  <field name="linked_docs" type="list">Supporting documentation URLs</field>
  <field name="constraints" type="list">Explicit limitations</field>
</skill>
