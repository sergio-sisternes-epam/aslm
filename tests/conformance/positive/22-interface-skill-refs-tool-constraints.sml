<!-- Interface with skill refs and tool constraints -->
<skill define="interface" name="brain-query">
  <param name="question" type="string" required="true">The question to answer</param>
  <returns name="answer" type="string">Citation-backed answer</returns>
  <reads>wiki/index.md, wiki/**/*.md</reads>
  <writes>raw/brain-questions.md</writes>
  <skill ref="dde" role="enforcement" />
  <tool allow="rename_session,send_session_message,view,edit,bash" />
</skill>
