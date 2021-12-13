import { Workspace, BN, NearAccount, captureError, toYocto, tGas } from 'near-workspaces-ava';
import { workspace, initStaking, initTestToken, STORAGE_PER_BYTE } from './utils';

async function proposeBounty(alice: NearAccount, dao: NearAccount) {  
    const bounty = {
        description: 'test_bounties',
        //token: alice,
        amount: '19000000000000000000000000',
        times: 3,
        max_deadline: '1925376849430593581'
    }
    const proposalId: number = await alice.call(dao, 'add_proposal', {
        proposal: {
            description: 'add_new_bounty',
            kind: {
                AddBounty: {
                    bounty
                }
            }
        },
    },
        { 
            attachedDeposit: toYocto('1') 
        }
    )
    return proposalId;
}

async function voteOnBounty(root: NearAccount, dao: NearAccount, proposalId: number) {
    await root.call(dao, 'act_proposal', 
    {
        id: proposalId,
        action: 'VoteApprove'
    })
}

async function claimBounty(alice: NearAccount, dao: NearAccount, proposalId: number) {
    await alice.call(dao, 'bounty_claim', 
    {
        id: proposalId,
        deadline: '1925376849430593581'

    },
    { 
        attachedDeposit: toYocto('1') 
    })
}

async function doneBounty(alice: NearAccount, dao: NearAccount, proposalId: number) {
    await alice.call(dao, 'bounty_done', 
    {
        id: proposalId,
        description: 'This bounty is done'

    },
    { 
        attachedDeposit: toYocto('1') 
    })
}


workspace.test('Bounty workflow', async (test, {alice, root, dao }) => {
    const proposalId = await proposeBounty(alice, dao);
    await voteOnBounty(root, dao, proposalId);
    await claimBounty(alice, dao, proposalId);
    console.log('Before bounty_done:');
    console.log(await dao.view('get_bounty_claims', { account_id: alice }));
    await doneBounty(alice, dao, proposalId);
    console.log('After bounty_done:');
    console.log(await dao.view('get_bounty_claims', { account_id: alice }));
    console.log('Before act_proposal, voting on the bounty:')
    console.log(await dao.view('get_proposal', { id: proposalId + 1 }));
    await voteOnBounty(root, dao, proposalId + 1);
    console.log('After act_proposal, voting on the bounty:')
    console.log(await dao.view('get_proposal', { id: proposalId + 1 }));
    
});

workspace.test('Bounty claim', async (test, {alice, root, dao }) => {
    const proposalId = await proposeBounty(alice, dao);
    await voteOnBounty(root, dao, proposalId);
    await claimBounty(alice, dao, proposalId);
});

workspace.test('Bounty done', async (test, {alice, root, dao }) => {
    const regCost = STORAGE_PER_BYTE.mul(new BN(16));

    const testToken = await initTestToken(root);
    const staking = await initStaking(root, dao, testToken);

    proposeBounty(alice, dao);

    let errorString = await captureError(async () =>
        staking.call(dao, 'bounty_done', 
        {
            id: 1,
            account_id: alice,
            description: 'new_bounty_done'
        },
            { attachedDeposit: regCost }));
    //test.regex(errorString, /ERR_BOUNTY_DONE_MUST_BE_SELF/);
});

workspace.test('Bounty giveup', async (test, {alice, root, dao }) => {
    proposeBounty(alice, dao);
});