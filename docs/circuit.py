# Citcuit pseudocode

# Data structures

struct op:
    
    # operation data
    tx_type:        # type of transaction, see the list: https://docs.google.com/spreadsheets/d/1ejK1MJfVehcwjgjVDFD3E2k1EZ7auqbG_y0DKidS9nA/edit#gid=0
    chunk:          # op chunk number (0..3)
    pubdata_chunk:  # current chunk of the pubdata (always 8 bytes)
    args:           # arguments for the operation
    
    # Merkle branches
    lhs:            # left Merkle branch data
    rhs:            # right Merkle branch data
    clear_subtree:  # bool: instruction to clear the account subtree in the current branch

    # precomputed witness:
    a:              # depends on the optype, used for range checks
    b:              # depends on the optype, used for range checks
    new_root:       # new state root after the operation is applied
    account_path:   # Merkle path witness for the account in the current branch
    subtree_path:   # Merkle path witness for the subtree in the current branch

struct cur:         # current Merkle branch data

struct computed:
    last_chunk:             # bool: whether the current chunk is the last one in sequence
    pubdata:                # pubdata accumulated over all chunks
    subtractable:           # wheather a >= b
    new_pubkey_hash:        # hash of the new pubkey, truncated to 20 bytes (used only for deposits)
    cosigner_pubkey_hash:   # hash of the cosigner pubkey in the current branch, truncated to 20 bytes


# Circuit functions

def circuit:

    running_hash := initial_hash
    current_root := last_state_root

    prev.lhs := { 0, ... } 
    prev.rhs := { 0, ... } 
    prev.chunk := 0
    prev.new_root := 0

    for op in operations:

        # enfore correct bitlentgh for every input in witness
        # TODO: for this create a macro gadget via struct member annotations
        verify_bitlength(op)

        # check and prepare data
        verify_correct_chunking(op, computed)
        accumulate_sha256(op.pubdata_chunk)
        accumulate_pubdata(op, computed)

        # prepare Merkle branch
        cur := select_branch(op, computed)
        computed.cosigner_pubkey_hash := hash(cur.cosigner_pubkey)

        # check initial Merkle paths, before applying the operation
        op.clear_account := False
        state_root := verify_merkle_paths(op, cur, computed, check_intersection = False)
        enforce state_root == current_root

        # check validity and perform state updates for the current branch by modifying `cur` struct
        execute_op(op, cur, computed)

        # check final Merkle paths after applying the operation
        new_root := verify_merkle_paths(op, cur, computed, check_intersection = True)

        # NOTE: this is checked separately for each branch side, and we already enforced 
        # that `op.new_root` remains unchanged for both by enforcing that it is shared by all chunks
        enforce new_root == op.new_root

        # update global state root on the last op chunk
        if computed.last_chunk:
            current_root = new_root
        
        # update `prev` references
        # TODO: need a gadget to copy struct members one by one
        prev.rhs = op.rhs
        prev.lhs = op.lhs
        prev.args = op.args
        prev.new_root = op.new_root
        prev.chunk = op.chunk

    # final checks after the loop end
    enforce current_root == new_state_root
    enforce running_hash == pubdata_hash
    enforce last_chunk # any operation should close with the last chunk


# make sure that operation chunks are passed correctly
def verify_correct_chunking(op, computed):

    # enforce chunk sequence correctness
    enforce (op.chunk == 0) or (op.chunk == prev.chunk + 1) # ensure that chunks come in sequence 
    max_chunks := switch op.tx_type
        deposit => 4,
        transfer_to_new=> 1,
        transfer => 2,
        # ...and so on
    enforce op.chunk < max_chunks # 4 constraints
    computed.last_chunk = op.chunk == max_chunks-1 # flag to mark the last op chunk

    # enforce that all chunks share the same witness:
    #   - `op.args` for the common arguments of the operation
    #   - `op.lhs` and `op.rhs` for left and right Merkle branches
    #   - `new_root` of the state after the operation is applied
    correct_inputs := 
        op.chunk == 0 # skip check for the first chunk
        or (
            prev.args == op.args and 
            prev.lhs == op.lhs and 
            prev.rhs == op.rhs and
            prev.new_root == op.new_root
        ) # TODO: need a gadget for logical equality which works with structs

    enforce correct_inputs


# accumulate pubdata from multiple chunks
def accumulate_pubdata(op, computed):
    computed.pubdata =  
        if op.chunk == 0:
            op.pubdata_chunk # initialize from the first chunk
        else:
            computed.pubdata << 8 + op.pubdata_chunk


# determine the Merkle branch side (0 for LHS, 1 for RHS) and set `cur` for the current Merkle branch
def select_branch(op, computed):
   
    op.current_side := LHS if op.tx_type == 'deposit' else op.chunk

    # TODO: need a gadget for conditional swap applied to each struct member:
    cur := op.lhs if current_side == LHS else op.rhs

    return cur

def verify_merkle_paths(op, cur, computed, check_intersection):

    balances_root := merkle_root(token, op.balances_path, cur.balance)

    subaccount_data := (cur.leaf_balance, cur.leaf_nonce, cur.creation_nonce, computed.cosigner_pubkey_hash, cur.cosigner_balance, cur.token)
    subaccounts_root := merkle_root(token, op.balances_path, subaccount_data)
    subtree_hash := hash(balances_root, subaccounts_root)

    subtree_root := EMPTY_SUBTREE if clear_subtree else subtree_hash

    account_data := hash(cur.owner_pub_key, cur.subtree_root, cur.account_nonce)

    intersection_path := intersection(op.account_path, cur.account, lhs.account, rhs.account, lhs.intersection_hash, rhs.intersection_hash)
    path_witness := intersection_path if check_intersection else op.account_path
    state_root := merkle_root(cur.account, path_witness, account_data)

    return state_root


# verify operation and execute state updates
def execute_op(op, cur, computed):

    # universal range check; a and b are different depending on the op

    computed.subtractable := op.a >= op.b

    # unpack floating point values and hashes

    op.args.amount  := unpack(op.args.amount_packed)
    op.args.fee     := unpack(op.args.fee_packed)

    # some operations require tighter amount packing (with less precision)

    computed.compact_amount_correct := op.args.amount == op.args.compact_amount * 256

    # new pubkey hash for deposits

    computed.new_pubkey_hash := hash(cur.new_pubkey)

    # signature check

    # NOTE: signature check must always be valid, but msg and signer can be phony
    enforce check_sig(cur.sig_msg, cur.signer_pubkey)

    # execute operations

    op_valid := False

    op_valid = op_valid or transfer_to_new(op, cur, computed)
    op_valid = op_valid or deposit(op, cur, computed)
    op_valid = op_valid or close_account(op, cur, computed)
    op_valid = op_valid or partial_exit(op, cur, computed)
    op_valid = op_valid or escalation(op, cur, computed)
    op_valid = op_valid or op.tx_type == 'noop'

    # `op` MUST be one of the operations and MUST be valid

    enforce op_valid


def transfer_to_new(op, cur, computed):
    # transfer_to_new validation is split into lhs and rhs; pubdata is combined from both branches

    lhs_valid :=
        op.tx_type == 'transfer_to_new'

        # here we process the first chunk
        and op.chunk == 0

        # sender is using a token balance, not subaccount
        and lhs.leaf_is_token

        # sender authorized spending and recepient
        and lhs.sig_msg == hash('transfer_to_new', lhs.account, lhs.leaf_index, lhs.account_nonce, op.args.amount_packed, op.args.fee_packed, cur.new_pubkey)

        # sender is account owner
        and lhs.signer_pubkey == cur.owner_pub_key

        # sender has enough balance: we checked above that `op.a >= op.b`
        # NOTE: no need to check overflow for `amount + fee` because their bitlengths are enforced]
        and computed.subtractable and (op.a == cur.leaf_balance) and (op.b == (op.args.amount + op.args.fee) )

    # NOTE: updating the state is done by modifying data in the `cur` branch
    if lhs_valid:
        cur.leaf_balance = cur.leaf_balance - (op.args.amount + op.args.fee)
        cur.account_nonce = cur.account_nonce + 1

    rhs_valid := 
        op.tx_type == 'transfer_to_new'

        # here we process the second (last) chunk
        and op.chunk == 1

        # compact amount is passed to pubdata for this operation
        and computed.compact_amount_correct

        # pubdata contains correct data from both branches, so we verify it agains `lhs` and `rhs`
        and pubdata == (op.tx_type, lhs.account, lhs.leaf_index, lhs.compact_amount, cur.new_pubkey_hash, rhs.account, rhs.fee)

        # new account branch is empty
        and (rhs.owner_pub_key, rhs.subtree_root, rhs.account_nonce) == EMPTY_ACCOUNT

        # deposit is into a token balance, not subaccount
        and rhs.leaf_is_token

        # sender signed the same recepient pubkey of which the hash was passed to public data
        and lhs.new_pubkey == rhs.new_pubkey

    if rhs_valid:
        cur.leaf_balance = op.args.amount
    
    return lhs_valid or rhs_valid


def deposit(op, cur, computed):

    ignore_pubdata := not last_chunk
    tx_valid := 
        op.tx_type == 'deposit'
        and (ignore_pubdata or pubdata == (cur.account, cur.leaf_index, args.compact_amount, cur.new_pubkey_hash, args.fee))
        and (cur.account_pubkey, cur.subtree_root, cur.account_nonce) == EMPTY_ACCOUNT
        and cur.leaf_is_token
        and computed.compact_amount_correct
        and computed.subtractable and (op.a == op.args.amount) and (op.b == op.args.fee )

    if tx_valid:
        cur.leaf_balance = op.args.amount - op.args.fee

    return tx_valid

def close_account(op, cur, computed):
    
    tx_valid :=
        op.tx_type == 'full_exit'
        and pubdata == (cur.account, cur.subtree_root)
        # TODO: check user signature

    if tx_valid:
        cur.owner_pub_key = 0
        cur.account_nonce = 0
        op.clear_subtree = True
    
    return tx_valid


def partial_exit(op, cur, computed):

    tx_valid := 
        op.tx_type == 'partial_exit'
        and computed.compact_amount_correct
        and pubdata == (op.tx_type, cur.account, cur.leaf_index, op.args.amount, op.args.fee)
        and subtractable
        and cur.leaf_is_token
        and cur.sig_msg == ('partial_exit', cur.account, cur.leaf_index, cur.account_nonce, cur.amount, cur.fee)
        and cur.signer_pubkey == cur.owner_pub_key

    if tx_valid:
        cur.leaf_balance = cur.leaf_balance - (op.args.amount + op.args.fee)
        cur.account_nonce = cur.leaf_nonce + 1
    
    return tx_valid


def escalation(op, cur, computed):

    tx_valid := 
        op.tx_type == 'escalation'
        and pubdata == (op.tx_type, cur.account, cur.leaf_index, cur.creation_nonce, cur.leaf_nonce)
        and not cur.leaf_is_token
        and cur.sig_msg == ('escalation', cur.account, cur.leaf_index, cur.creation_nonce)
        (cur.signer_pubkey == cur.owner_pub_key or cur.signer_pubkey == cosigner_pubkey)

    if tx_valid:
        cur.leaf_balance = 0
        cur.leaf_nonce = 0
        cur.creation_nonce = 0
        cur.cosigner_pubkey_hash = EMPTY_HASH
    
    return tx_valid

def transfer(op, cur, computed):

    lhs_valid :=
        op.tx_type == 'transfer'
        and op.chunk == 0
        and cur.leaf_is_token
        and lhs.sig_msg == ('transfer', lhs.account, lhs.leaf_index, lhs.account_nonce, op.args.amount_packed, op.args.fee_packed, rhs.account_pubkey)
        and lhs.signer_pubkey == cur.owner_pub_key
        and computed.subtractable and (op.a == cur.leaf_balance) and (op.b == (op.args.amount + op.args.fee) )

    if lhs_valid:
        cur.leaf_balance = cur.leaf_balance - (op.args.amount + op.args.fee)
        cur.account_nonce = cur.account_nonce + 1

    rhs_valid := 
        op.tx_type == 'transfer'
        and op.chunk == 1
        and pubdata == (op.tx_type, lhs.account, lhs.leaf_index, op.args.amount, rhs.account, op.args.fee)
        and cur.leaf_is_token

    if rhs_valid:
        cur.leaf_balance = op.args.amount

    return lhs_valid or rhs_valid

# Subaccount operations

def create_subaccount(op, cur, computed):

    # on the LHS we have cosigner, no need to do anything; so we only process the RHS here

    # tx_valid :=
    #     op.tx_type == 'create_subaccount'
    #     and op.chunk == 1
    #     and cur.leaf_is_token
    #     and lhs.sig_msg == ('create_subaccount', lhs.account, lhs.leaf_index, lhs.account_nonce, op.args.amount_packed, op.args.fee_packed, rhs.account_pubkey)
    #     and lhs.signer_pubkey == cur.owner_pub_key
    #     and computed.subtractable and (op.a == cur.leaf_balance) and (op.b == (op.args.amount + op.args.fee) )

    # if tx_valid:
    #     cur.leaf_balance = cur.leaf_balance - (op.args.amount + op.args.fee)
    #     cur.account_nonce = cur.account_nonce + 1

    # rhs_valid := 
    #     op.tx_type == 'create_subaccount'
    #     and op.chunk == 1
    #     and pubdata == (op.tx_type, lhs.account, lhs.leaf_index, op.args.amount, rhs.account, op.args.fee)
    #     and cur.leaf_is_token

    # if rhs_valid:
    #     cur.leaf_balance = op.args.amount

    # return lhs_valid or rhs_valid