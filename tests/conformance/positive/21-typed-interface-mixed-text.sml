<!-- Interface definition mixing prose text and typed declarations -->
<skill define="interface" name="code-analyser">
  Analyse source code for quality issues and return a structured report.
  <param name="target" type="path" required="true">Path to analyse</param>
  <param name="severity" type="enum" values="low|medium|high|critical" default="medium">Minimum severity</param>
  <returns name="report" type="string">Structured quality report</returns>
  <reads>src/**/*.rs</reads>
</skill>
