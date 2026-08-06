/* ==========================================================================
   RetinaX — Interactive Senior Frontend Logic & Cybernetic Canvas
   ========================================================================== */

document.addEventListener('DOMContentLoaded', () => {
  initIrisCanvas();
  initDemoTabs();
  initRBACSimulator();
  initZKSimulator();
  initAISimulator();
  initFHIRSimulator();
});

/* ==========================================================================
   1. Interactive Background Cybernetic Iris Animation
   ========================================================================== */
function initIrisCanvas() {
  const canvas = document.getElementById('iris-canvas');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');

  let width = canvas.width = window.innerWidth;
  let height = canvas.height = window.innerHeight;

  window.addEventListener('resize', () => {
    width = canvas.width = window.innerWidth;
    height = canvas.height = window.innerHeight;
  });

  let angle = 0;
  const nodes = [];
  const totalNodes = 40;

  for (let i = 0; i < totalNodes; i++) {
    nodes.push({
      radius: 120 + Math.random() * 200,
      angle: (i / totalNodes) * Math.PI * 2,
      speed: 0.002 + Math.random() * 0.003,
      size: 2 + Math.random() * 3
    });
  }

  function draw() {
    ctx.clearRect(0, 0, width, height);

    const centerX = width / 2;
    const centerY = height / 2;

    angle += 0.003;

    // Draw central glowing iris ring
    ctx.save();
    ctx.translate(centerX, centerY);
    ctx.rotate(angle * 0.5);

    // Glowing outer ring
    ctx.beginPath();
    ctx.arc(0, 0, 180, 0, Math.PI * 2);
    ctx.strokeStyle = 'rgba(0, 242, 254, 0.15)';
    ctx.lineWidth = 2;
    ctx.stroke();

    // Inner dashed cyber ring
    ctx.beginPath();
    ctx.arc(0, 0, 130, 0, Math.PI * 2);
    ctx.setLineDash([8, 12]);
    ctx.strokeStyle = 'rgba(127, 0, 255, 0.25)';
    ctx.lineWidth = 1.5;
    ctx.stroke();

    // Draw cyber nodes and network connections
    ctx.setLineDash([]);
    nodes.forEach((node, idx) => {
      node.angle += node.speed;
      const x = Math.cos(node.angle) * node.radius;
      const y = Math.sin(node.angle) * node.radius;

      // Draw node point
      ctx.beginPath();
      ctx.arc(x, y, node.size, 0, Math.PI * 2);
      ctx.fillStyle = idx % 2 === 0 ? 'rgba(0, 242, 254, 0.6)' : 'rgba(127, 0, 255, 0.6)';
      ctx.fill();

      // Connect to center if close
      if (idx % 4 === 0) {
        ctx.beginPath();
        ctx.moveTo(0, 0);
        ctx.lineTo(x, y);
        ctx.strokeStyle = 'rgba(0, 242, 254, 0.05)';
        ctx.stroke();
      }
    });

    ctx.restore();

    requestAnimationFrame(draw);
  }

  draw();
}

/* ==========================================================================
   2. Tab Switching Logic
   ========================================================================== */
function initDemoTabs() {
  const tabs = document.querySelectorAll('.demo-tab');
  const panels = document.querySelectorAll('.demo-panel');

  tabs.forEach(tab => {
    tab.addEventListener('click', () => {
      tabs.forEach(t => t.classList.remove('active'));
      panels.forEach(p => p.classList.remove('active'));

      tab.classList.add('active');
      const targetId = `panel-${tab.dataset.tab}`;
      const targetPanel = document.getElementById(targetId);
      if (targetPanel) targetPanel.classList.add('active');
    });
  });
}

/* ==========================================================================
   3. RBAC Simulator
   ========================================================================== */
function initRBACSimulator() {
  const durSlider = document.getElementById('dur-slider');
  const durVal = document.getElementById('dur-val');
  const doctorSelect = document.getElementById('select-doctor');
  const grantBtn = document.getElementById('btn-grant-access');
  const previewCode = document.getElementById('preview-rbac-code');

  if (durSlider && durVal) {
    durSlider.addEventListener('input', (e) => {
      durVal.textContent = e.target.value;
      updateRBACPreview();
    });
  }

  if (doctorSelect) {
    doctorSelect.addEventListener('change', updateRBACPreview);
  }

  if (grantBtn) {
    grantBtn.addEventListener('click', () => {
      grantBtn.textContent = '⚡ Executing Soroban require_auth()...';
      grantBtn.style.opacity = '0.7';

      setTimeout(() => {
        grantBtn.textContent = '✅ Access Granted On-Chain!';
        grantBtn.style.opacity = '1';
        updateRBACPreview(true);

        setTimeout(() => {
          grantBtn.textContent = 'Execute Soroban Auth';
        }, 2500);
      }, 700);
    });
  }

  function updateRBACPreview(executed = false) {
    const doctor = doctorSelect ? doctorSelect.value : 'GAB...DR_SMITH_OPTOMETRY';
    const hours = durSlider ? durSlider.value : '24';
    const statusStr = executed ? 'SUCCESS (LEDGER_ENFORCED)' : 'PENDING_SIGNATURE';

    if (previewCode) {
      previewCode.textContent = `// Soroban Call: contracts/vision_records::grant_access()
fn grant_access(env: Env, patient: Address, doctor: Address, ttl: u64) {
    patient.require_auth(); // Validated GDC...PATIENT_KEY_99X
    
    // Target Clinic: ${doctor}
    // Duration TTL: ${hours} Hours (${hours * 3600} Seconds)
    let grant = AccessGrant {
        granted_at: env.ledger().timestamp(),
        expires_at: env.ledger().timestamp() + ${hours * 3600},
        active: true,
    };
    
    env.storage().persistent().set(&(patient, doctor), &grant);
    env.events().publish(("access", "granted"), (patient, doctor));
}

// Transaction Status: ${statusStr}`;
    }
  }
}

/* ==========================================================================
   4. ZK Proof Simulator
   ========================================================================== */
function initZKSimulator() {
  const acuitySlider = document.getElementById('acuity-slider');
  const acuityVal = document.getElementById('acuity-val');
  const genZkBtn = document.getElementById('btn-gen-zk');
  const previewZkCode = document.getElementById('preview-zk-code');

  if (acuitySlider && acuityVal) {
    acuitySlider.addEventListener('input', (e) => {
      const val = e.target.value;
      acuityVal.textContent = `20/${val}`;
      updateZKPreview();
    });
  }

  if (genZkBtn) {
    genZkBtn.addEventListener('click', () => {
      genZkBtn.textContent = '🛡️ Generating Groth16 Proof...';

      setTimeout(() => {
        genZkBtn.textContent = '✅ Proof Verified Valid!';
        updateZKPreview(true);

        setTimeout(() => {
          genZkBtn.textContent = 'Generate Groth16 zk-SNARK';
        }, 2500);
      }, 800);
    });
  }

  function updateZKPreview(generated = false) {
    const score = acuitySlider ? acuitySlider.value : '20';
    const isValid = parseInt(score) <= 40;
    const proofHex = generated ? '0x98f21a007b82f10d9e4a8b71...' : '0xPREPARED_ZK_PROOF_BYTES';

    if (previewZkCode) {
      previewZkCode.textContent = `{
  "circuit": "contracts/zk_verifier::visual_acuity_proof",
  "patient_private_acuity": "20/${score}",
  "public_threshold": "Visual Acuity <= 20/40",
  "proof_bytes": "${proofHex}",
  "evaluation_result": ${isValid ? '"PASS_VERIFIED"' : '"FAIL_REQUIREMENT_NOT_MET"'},
  "status": "${generated ? 'ONCHAIN_VERIFIED' : 'READY_TO_SUBMIT'}",
  "patient_identity_revealed": false
}`;
    }
  }
}

/* ==========================================================================
   5. AI Oracle Rotation Simulator
   ========================================================================== */
function initAISimulator() {
  const aiStatusSelect = document.getElementById('select-ai-status');
  const testAiBtn = document.getElementById('btn-test-ai');
  const previewAiCode = document.getElementById('preview-ai-code');

  if (testAiBtn) {
    testAiBtn.addEventListener('click', () => {
      const status = aiStatusSelect ? aiStatusSelect.value : 'healthy';

      testAiBtn.textContent = '🤖 Evaluating Diagnostic Oracles...';

      setTimeout(() => {
        testAiBtn.textContent = 'Execute Diagnostic Check';

        if (status === 'timeout') {
          if (previewAiCode) {
            previewAiCode.textContent = `[AI_INTEGRATION] Primary Provider "AI_Vision_Alpha" TIMEOUT (500ms Exceeded).
[FAILOVER_TRIGGERED] Executing contracts/ai_integration::rotate_provider()
[PROVIDER_ROTATION] Degrading "AI_Vision_Alpha" Weight: 100 -> 0 (Status: Paused)
[SECONDARY_PROMOTED] Activated Secondary Oracle: "AI_Vision_Beta" (Weight: 90)
[DIAGNOSTIC_RESULT] Analysis Complete via Backup Node.
[EVENT_EMITTED] ProviderRotated(Primary: "AI_Vision_Beta", Reason: SLA_Timeout)`;
          }
        } else {
          if (previewAiCode) {
            previewAiCode.textContent = `[AI_INTEGRATION] Primary Provider Active: "AI_Vision_Alpha" (Weight: 100)
[ORACLE_CALL] Processing Retinal Scan CID: ipfs://QmX9z82...
[DIAGNOSTIC_RESULT] Diagnostics Confirmed: No Diabetic Retinopathy Detected.
[LATENCY] Response Time: 42ms (Within 100ms SLA Window)
[EVENT_EMITTED] ProviderStatusChecked(Active, Weight: 100)`;
          }
        }
      }, 600);
    });
  }
}

/* ==========================================================================
   6. FHIR v4 Converter Simulator
   ========================================================================== */
function initFHIRSimulator() {
  const fhirTypeSelect = document.getElementById('select-fhir-type');
  const convertFhirBtn = document.getElementById('btn-convert-fhir');
  const previewFhirCode = document.getElementById('preview-fhir-code');

  if (convertFhirBtn) {
    convertFhirBtn.addEventListener('click', () => {
      const type = fhirTypeSelect ? fhirTypeSelect.value : 'refraction';

      convertFhirBtn.textContent = '🏥 Mapping to FHIR v4 JSON...';

      setTimeout(() => {
        convertFhirBtn.textContent = 'Generate FHIR v4 Payload';

        if (type === 'iop') {
          if (previewFhirCode) {
            previewFhirCode.textContent = `{
  "resourceType": "Observation",
  "status": "final",
  "code": {
    "coding": [{
      "system": "http://loinc.org",
      "code": "56844-4",
      "display": "Intraocular pressure by Tonometry"
    }]
  },
  "subject": { "reference": "Patient/GDC...PATIENT_KEY_99X" },
  "valueQuantity": {
    "value": 14.5,
    "unit": "mmHg",
    "system": "http://unitsofmeasure.org"
  },
  "effectiveDateTime": "2026-08-06T23:45:00Z"
}`;
          }
        } else {
          if (previewFhirCode) {
            previewFhirCode.textContent = `{
  "resourceType": "DiagnosticReport",
  "status": "final",
  "code": {
    "coding": [{
      "system": "http://loinc.org",
      "code": "28886-0",
      "display": "Refraction examination"
    }]
  },
  "subject": { "reference": "Patient/GDC...PATIENT_KEY_99X" },
  "conclusion": "Normal refraction. Prescription: Right Eye -1.25 SPH, Left Eye -1.00 SPH",
  "effectiveDateTime": "2026-08-06T23:45:00Z"
}`;
          }
        }
      }, 500);
    });
  }
}
