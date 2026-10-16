// Classify only. Execution and its original-request ownership stay on the Mac.
export async function classify(body, ai) {
  const state=JSON.parse(body);
  if (!state || Array.isArray(state) || Object.keys(state).sort().join(',')!=='recent_conversation,roles,user_request'
      || typeof state.user_request!=='string' || !state.user_request.trim()
      || typeof state.recent_conversation!=='string' || !state.roles || Array.isArray(state.roles)
      || Object.keys(state.roles).sort().join(',')!=='rich,stu'
      || ['rich','stu'].some(role=>typeof state.roles[role]!=='string' || !state.roles[role].trim())) throw new SyntaxError('invalid_request');
  if (!ai) throw Error('classifier_unavailable');
  const result=await ai.run('@cf/cloudflare/clef-flash',{
    model:'clef-flash',state,questions:{handler:{type:'choice',
      instructions:'Who should handle this user request, based on the expected time to complete it and the supplied agent roles and conversation context?',
      criteria:{
        rich:'Rich can answer or complete the request within a few seconds using available context or a quick lookup.',
        stu:'Completing the request requires longer work, investigation or orchestration.',
        uncertain:'Available context is insufficient to decide reliably.',
      },
    }},
  });
  const answer=result?.answers?.handler;
  if (result?.model!=='clef-flash' || answer?.type!=='choice' || !['rich','stu','uncertain'].includes(answer.choice)
      || !answer.probabilities || ['rich','stu','uncertain'].some(key=>!Number.isFinite(answer.probabilities[key])
        || answer.probabilities[key]<0 || answer.probabilities[key]>1)) throw Error('invalid_classifier_result');
  return {model:'clef-flash',handler:answer.choice};
}
