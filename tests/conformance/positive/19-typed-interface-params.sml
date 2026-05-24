<!-- Interface definition with typed param declarations -->
<skill define="interface" name="brain-query">
  <param name="question" type="string" required="true">The question to answer</param>
  <param name="format" type="enum" values="markdown|comparison-table|slide-deck|chart" default="markdown">Output format</param>
  <param name="caller_session_id" type="string" required="false">Set by brain-responder</param>
  <returns name="answer" type="string">Citation-backed answer</returns>
  <returns name="quality" type="enum" values="answered|partial|unanswered" />
  <reads>wiki/index.md, wiki/**/*.md</reads>
  <writes>raw/brain-questions.md</writes>
</skill>
