<!-- Implementation with wrapping skill ref containing nodes -->
<skill define="interface" name="brain-query">
  <param name="question" type="string" required="true">The question to answer</param>
  <returns name="answer" type="string">Citation-backed answer</returns>
</skill>

<skill define="implementation" name="brain-handler" implements="brain-query">
  <skill ref="dde" role="enforcement">
    ```mermaid
    flowchart LR
        A[Rename] --> B[ReadIndex] --> C[Synthesise] --> D[Reply]
    ```

    <node name="Rename" type="tool">
      <tool use="rename_session" />
      Rename this session with pattern `query-{brief-description}`
    </node>
    <node name="ReadIndex" type="tool">
      <tool use="view" />
      Read wiki/index.md to find candidate pages
    </node>
    <node name="Synthesise" type="prompt">
      Drill into candidate pages, synthesise citation-backed answer
    </node>
    <node name="Reply" type="tool">
      <tool use="send_session_message" />
      Send the answer back to the caller
    </node>
  </skill>
</skill>
