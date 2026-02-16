# Trusted Autonomy — Terms of Use & Disclaimer

**Version**: 1.0 (2026-02-11)

By using, installing, or running Trusted Autonomy ("the Software"), you acknowledge
and agree to the following terms. If you do not agree, do not use the Software.

---

## 1. Alpha Software Disclaimer

This Software is provided in **alpha** state. It is under active development
and has **not been audited for security, correctness, or reliability**.

- **Do not** use this Software for critical, production, or irreversible operations.
- **Do not** trust this Software with secrets, credentials, or sensitive data.
- **Do not** rely on this Software as a sole security control.

The staging and review model is designed to reduce risk but does not eliminate it.
Agents run with your system permissions inside a staging directory copy. There is
no sandbox isolation in the current release.

## 2. No Warranty

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, AND NONINFRINGEMENT.

THE AUTHORS AND COPYRIGHT HOLDERS MAKE NO REPRESENTATIONS OR WARRANTIES
REGARDING THE ACCURACY, RELIABILITY, COMPLETENESS, OR TIMELINESS OF THE
SOFTWARE OR ITS OUTPUT.

IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES, OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT, OR
OTHERWISE, ARISING FROM, OUT OF, OR IN CONNECTION WITH THE SOFTWARE OR THE
USE OR OTHER DEALINGS IN THE SOFTWARE.

## 3. Limitation of Liability

TO THE MAXIMUM EXTENT PERMITTED BY APPLICABLE LAW, IN NO EVENT SHALL THE
AUTHORS, CONTRIBUTORS, OR COPYRIGHT HOLDERS BE LIABLE FOR ANY INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, CONSEQUENTIAL, OR PUNITIVE DAMAGES
(INCLUDING BUT NOT LIMITED TO LOSS OF DATA, LOSS OF PROFITS, BUSINESS
INTERRUPTION, OR LOSS OF GOODWILL), REGARDLESS OF THE CAUSE OF ACTION OR
THE THEORY OF LIABILITY, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

THE AGGREGATE LIABILITY OF THE AUTHORS AND CONTRIBUTORS FOR ALL CLAIMS
ARISING FROM OR RELATED TO THE SOFTWARE SHALL NOT EXCEED THE AMOUNT YOU
PAID FOR THE SOFTWARE (WHICH MAY BE ZERO).

## 4. Indemnification

You agree to indemnify, defend, and hold harmless the authors, contributors,
and copyright holders of the Software from and against any and all claims,
damages, losses, costs, and expenses (including reasonable legal fees) arising
from or related to:

- Your use of the Software
- Any output, actions, or effects produced by AI agents operating under the Software
- Your violation of these terms
- Your violation of any applicable law or regulation

## 5. AI Agent Conduct

The Software mediates AI agent actions through staging and review workflows.
However:

- You are solely responsible for the actions you approve and apply.
- The Software does not guarantee that AI agents will behave as intended.
- Review all proposed changes carefully before approval.
- The staging model reduces but does not eliminate the risk of unintended effects.

## 6. Data and Privacy

The Software operates locally on your machine. It does not transmit data to
the authors or any third party. However:

- AI agents invoked by the Software (Claude, Codex, etc.) are subject to their
  own providers' terms of service and privacy policies.
- API keys and credentials you provide are used to authenticate with those
  third-party services.
- The Software does not store, transmit, or access your API keys beyond the
  local process environment.

## 7. Acceptance

By running `ta` (the CLI) or any component of the Software, you confirm that
you have read, understood, and agree to these terms.

First-time users will be prompted to accept these terms before the Software
will operate. Acceptance is recorded locally at `~/.config/ta/terms.json`.
If these terms are updated in a future release, you will be prompted to
re-accept before continuing.

## 8. Governing License

This Software is licensed under the **Apache License, Version 2.0**. These
terms supplement (and do not replace) the Apache 2.0 license. In case of
conflict, the Apache 2.0 license governs for licensing purposes; these terms
govern for use and liability purposes.

See the [LICENSE](LICENSE) file for the full Apache 2.0 license text.

---

**By proceeding, you agree to these terms.**
