<!-- ERROR: duplicate node names in implementation -->
<skill define="interface" name="test-skill">
  <param name="input" type="string" required="true">Input data</param>
</skill>

<skill define="implementation" name="test-impl" implements="test-skill">
  <node name="Step1" type="tool">
    <tool use="view" />
    Read something
  </node>
  <node name="Step1" type="prompt">
    Duplicate name should error
  </node>
</skill>
